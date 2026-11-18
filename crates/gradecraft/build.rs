//! Bettet bei Windows-Builds Programm-Icon und Versionsinfos in die .exe ein
//! (funktioniert nativ unter Windows und beim Cross-Compiling mit MinGW).

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "GradeCraft");
        res.set(
            "FileDescription",
            "GradeCraft – Notenverwaltung für Lehrkräfte",
        );
        res.set("CompanyName", "GradeCraft");
        res.set_language(0x0407); // Deutsch (Deutschland)
        res.compile()
            .expect("Windows-Ressourcen konnten nicht eingebettet werden");
    }
}
