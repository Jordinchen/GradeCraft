; Inno-Setup-Skript für GradeCraft
; Wird von .github/workflows/release.yml aufgerufen:
;   iscc /DAppVersion=0.1.0 /DSourceExe=..\target\release\gradecraft.exe installer\gradecraft.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\target\release\gradecraft.exe"
#endif

[Setup]
; Feste AppId: neue Versionen aktualisieren die bestehende Installation
AppId={{6E2B6C1A-9C3F-4D8E-A7B1-5F0D2C8E9A41}
AppName=GradeCraft
AppVersion={#AppVersion}
AppVerName=GradeCraft {#AppVersion}
AppPublisher=GradeCraft
AppPublisherURL=https://github.com/jordinchen/gradecraft
AppSupportURL=https://github.com/jordinchen/gradecraft/issues
AppUpdatesURL=https://github.com/jordinchen/gradecraft/releases
DefaultDirName={autopf}\GradeCraft
DefaultGroupName=GradeCraft
DisableProgramGroupPage=yes
; Installation ohne Administratorrechte möglich (wichtig für Dienstgeräte)
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=..\dist
OutputBaseFilename=GradeCraft-{#AppVersion}-setup
SetupIconFile=..\crates\gradecraft\assets\icon.ico
UninstallDisplayIcon={app}\GradeCraft.exe
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
RestartApplications=yes
ChangesAssociations=yes

[Languages]
Name: "german"; MessagesFile: "compiler:Languages\German.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "assoc"; Description: "GradeCraft-Dateien (.gcdb) mit GradeCraft öffnen"; GroupDescription: "Dateizuordnung:"

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "GradeCraft.exe"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\GradeCraft"; Filename: "{app}\GradeCraft.exe"
Name: "{autodesktop}\GradeCraft"; Filename: "{app}\GradeCraft.exe"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\Classes\.gcdb"; ValueType: string; ValueName: ""; ValueData: "GradeCraft.Notendatei"; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\GradeCraft.Notendatei"; ValueType: string; ValueName: ""; ValueData: "GradeCraft-Notendatei"; Flags: uninsdeletekey; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\GradeCraft.Notendatei\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\GradeCraft.exe,0"; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\GradeCraft.Notendatei\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\GradeCraft.exe"" ""%1"""; Tasks: assoc

[Run]
Filename: "{app}\GradeCraft.exe"; Description: "{cm:LaunchProgram,GradeCraft}"; Flags: nowait postinstall skipifsilent
