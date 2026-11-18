//! Verschlüsselte Speicherung.
//!
//! Dateiformat (`.gcdb`):
//! ```text
//! "GRDCRAFT" (8) | Version u8 | m_cost u32 | t_cost u32 | p_cost u32 | Salt (16) | Nonce (12) | Ciphertext
//! ```
//! Der Schlüssel wird mit Argon2id aus dem Passwort abgeleitet, der Inhalt
//! (JSON) mit AES-256-GCM verschlüsselt. Der Header ist als „associated data“
//! gegen Manipulation geschützt.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngCore;
use zeroize::Zeroizing;

use crate::model::Database;

const MAGIC: &[u8; 8] = b"GRDCRAFT";
const FILE_VERSION: u8 = 1;
const HEADER_LEN: usize = 8 + 1 + 12 + 16 + 12;
pub const FILE_EXTENSION: &str = "gcdb";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Datei konnte nicht gelesen/geschrieben werden: {0}")]
    Io(#[from] std::io::Error),
    #[error("Keine GradeCraft-Datei oder Datei beschädigt")]
    Format,
    #[error("Falsches Passwort oder Datei beschädigt")]
    WrongPassword,
    #[error("Daten konnten nicht gelesen werden: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Schlüsselableitung fehlgeschlagen")]
    Kdf,
    #[error("Diese Datei wurde mit einer neueren GradeCraft-Version erstellt")]
    TooNew,
}

#[derive(Clone, Copy)]
struct KdfParams {
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        // 64 MiB, 3 Durchläufe – auch auf älteren Dienstlaptops < 1 s.
        Self {
            m_cost: 64 * 1024,
            t_cost: 3,
            p_cost: 1,
        }
    }
}

/// Eine geöffnete, entsperrte Datei. Hält den abgeleiteten Schlüssel im Speicher.
pub struct Vault {
    path: PathBuf,
    key: Zeroizing<[u8; 32]>,
    salt: [u8; 16],
    params: KdfParams,
}

fn derive_key(
    password: &str,
    salt: &[u8; 16],
    p: KdfParams,
) -> Result<Zeroizing<[u8; 32]>, StoreError> {
    let params =
        Params::new(p.m_cost, p.t_cost, p.p_cost, Some(32)).map_err(|_| StoreError::Kdf)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| StoreError::Kdf)?;
    Ok(key)
}

impl Vault {
    /// Legt eine neue, leere Datei an.
    pub fn create(
        path: impl Into<PathBuf>,
        password: &str,
        db: &Database,
    ) -> Result<Self, StoreError> {
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        let params = KdfParams::default();
        let key = derive_key(password, &salt, params)?;
        let vault = Vault {
            path: path.into(),
            key,
            salt,
            params,
        };
        vault.save(db)?;
        Ok(vault)
    }

    /// Öffnet und entschlüsselt eine vorhandene Datei.
    pub fn open(path: impl Into<PathBuf>, password: &str) -> Result<(Self, Database), StoreError> {
        let path = path.into();
        let bytes = fs::read(&path)?;
        if bytes.len() < HEADER_LEN || &bytes[..8] != MAGIC {
            return Err(StoreError::Format);
        }
        if bytes[8] > FILE_VERSION {
            return Err(StoreError::TooNew);
        }
        let u32_at = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
        let params = KdfParams {
            m_cost: u32_at(9),
            t_cost: u32_at(13),
            p_cost: u32_at(17),
        };
        // Schutz vor absurden Parametern in manipulierten Dateien
        if params.m_cost > 4 * 1024 * 1024 || params.t_cost > 100 || params.p_cost > 64 {
            return Err(StoreError::Format);
        }
        let salt: [u8; 16] = bytes[21..37].try_into().unwrap();
        let nonce = Nonce::from_slice(&bytes[37..49]);
        let key = derive_key(password, &salt, params)?;
        let cipher = Aes256Gcm::new_from_slice(key.as_ref()).map_err(|_| StoreError::Kdf)?;
        let plain = Zeroizing::new(
            cipher
                .decrypt(
                    nonce,
                    Payload {
                        msg: &bytes[HEADER_LEN..],
                        aad: &bytes[..HEADER_LEN],
                    },
                )
                .map_err(|_| StoreError::WrongPassword)?,
        );
        let mut db: Database = serde_json::from_slice(&plain)?;
        if db.format_version > crate::model::DB_FORMAT_VERSION {
            return Err(StoreError::TooNew);
        }
        db.normalize();
        Ok((
            Vault {
                path,
                key,
                salt,
                params,
            },
            db,
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Speichert verschlüsselt und atomar (temporäre Datei + Umbenennen).
    pub fn save(&self, db: &Database) -> Result<(), StoreError> {
        let bytes = self.encrypt(db)?;
        if let Some(dir) = self.path.parent()
            && !dir.as_os_str().is_empty()
        {
            fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension(format!("{FILE_EXTENSION}.tmp"));
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(&bytes)?;
            f.sync_all()?;
        }
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    /// Speichert unter neuem Pfad (gleiches Passwort).
    pub fn save_as(&mut self, path: impl Into<PathBuf>, db: &Database) -> Result<(), StoreError> {
        let old = std::mem::replace(&mut self.path, path.into());
        if let Err(e) = self.save(db) {
            self.path = old;
            return Err(e);
        }
        Ok(())
    }

    /// Exportiert eine verschlüsselte Kopie an einen anderen Ort.
    pub fn export_copy(&self, path: &Path, db: &Database) -> Result<(), StoreError> {
        fs::write(path, self.encrypt(db)?)?;
        Ok(())
    }

    pub fn change_password(&mut self, new_password: &str, db: &Database) -> Result<(), StoreError> {
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        let params = KdfParams::default();
        self.key = derive_key(new_password, &salt, params)?;
        self.salt = salt;
        self.params = params;
        self.save(db)
    }

    /// Prüft ein Passwort gegen den aktuellen Schlüssel (z. B. beim Entsperren).
    pub fn verify_password(&self, password: &str) -> bool {
        derive_key(password, &self.salt, self.params).is_ok_and(|k| *k == *self.key)
    }

    fn encrypt(&self, db: &Database) -> Result<Vec<u8>, StoreError> {
        let plain = Zeroizing::new(serde_json::to_vec(db)?);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let mut out = Vec::with_capacity(HEADER_LEN + plain.len() + 16);
        out.extend_from_slice(MAGIC);
        out.push(FILE_VERSION);
        out.extend_from_slice(&self.params.m_cost.to_le_bytes());
        out.extend_from_slice(&self.params.t_cost.to_le_bytes());
        out.extend_from_slice(&self.params.p_cost.to_le_bytes());
        out.extend_from_slice(&self.salt);
        out.extend_from_slice(&nonce);
        let cipher = Aes256Gcm::new_from_slice(self.key.as_ref()).map_err(|_| StoreError::Kdf)?;
        let ct = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: &plain,
                    aad: &out,
                },
            )
            .map_err(|_| StoreError::Kdf)?;
        out.extend_from_slice(&ct);
        Ok(out)
    }

    /// Ordner für Sicherungskopien: `<Datei-Ordner>/Sicherungen`.
    pub fn backup_dir(&self) -> PathBuf {
        self.path
            .parent()
            .unwrap_or(Path::new("."))
            .join("Sicherungen")
    }

    /// Legt eine (bereits verschlüsselte) Sicherungskopie der Datei auf der
    /// Festplatte an und behält nur die neuesten `keep` Kopien.
    pub fn backup(&self, keep: usize) -> Result<Option<PathBuf>, StoreError> {
        if keep == 0 || !self.path.exists() {
            return Ok(None);
        }
        let dir = self.backup_dir();
        fs::create_dir_all(&dir)?;
        let stem = self
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("gradecraft")
            .to_string();
        let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
        let target = dir.join(format!("{stem}_{stamp}.{FILE_EXTENSION}"));
        fs::copy(&self.path, &target)?;
        let prefix = format!("{stem}_");
        let mut existing: Vec<PathBuf> = fs::read_dir(&dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(FILE_EXTENSION))
            })
            .collect();
        existing.sort(); // Zeitstempel sortieren lexikografisch korrekt
        while existing.len() > keep {
            let _ = fs::remove_file(existing.remove(0));
        }
        Ok(Some(target))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "gc-test-{}-{}",
            std::process::id(),
            rand::random::<u32>()
        ));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn roundtrip_und_falsches_passwort() {
        let dir = tmpdir();
        let path = dir.join("noten.gcdb");
        let mut db = Database::new();
        db.school_years[0].name = "Test".into();
        Vault::create(&path, "geheim", &db).unwrap();
        let raw = fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("Test"));
        let (v, db2) = Vault::open(&path, "geheim").unwrap();
        assert_eq!(db2.school_years[0].name, "Test");
        assert!(v.verify_password("geheim"));
        assert!(!v.verify_password("falsch"));
        assert!(matches!(
            Vault::open(&path, "falsch"),
            Err(StoreError::WrongPassword)
        ));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn passwort_aendern_und_backup() {
        let dir = tmpdir();
        let path = dir.join("noten.gcdb");
        let db = Database::new();
        let mut v = Vault::create(&path, "a", &db).unwrap();
        v.change_password("b", &db).unwrap();
        assert!(Vault::open(&path, "a").is_err());
        assert!(Vault::open(&path, "b").is_ok());
        for _ in 0..3 {
            v.backup(2).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1100));
        }
        assert_eq!(fs::read_dir(v.backup_dir()).unwrap().count(), 2);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn manipulation_erkannt() {
        let dir = tmpdir();
        let path = dir.join("noten.gcdb");
        Vault::create(&path, "pw", &Database::new()).unwrap();
        let mut raw = fs::read(&path).unwrap();
        let n = raw.len();
        raw[n - 5] ^= 1;
        fs::write(&path, raw).unwrap();
        assert!(Vault::open(&path, "pw").is_err());
        fs::remove_dir_all(dir).ok();
    }
}
