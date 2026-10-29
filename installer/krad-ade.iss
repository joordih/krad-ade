; Windows installer for Krad ADE, built with Inno Setup 6:
;   cargo build --release -p krad-app
;   ISCC installer\krad-ade.iss
; Pass /DRelease=<folder> to package exes built into another target folder,
; and /DAppVersion=<version> to stamp another version (the release workflow).
; The setup lands in target\installer. It installs per user (no admin
; prompt) into %LOCALAPPDATA%\Programs\Krad ADE; krad-ade-agents.exe goes next
; to krad-ade.exe because the app puts its own folder on the PATH of the
; terminals it opens, which is how agents find the CLI. Shortcuts use the
; icon crates/krad-app/build.rs embeds in krad-ade.exe.

#define AppName "Krad ADE"
#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#define AppExe "krad-ade.exe"
#define AgentsExe "krad-ade-agents.exe"
#define Root ".."
#ifndef Release
  #define Release Root + "\target\release"
#endif
#define AppIcon "krad-ade.ico"
; What the app installed while it was called RMD ADE, under the same AppId.
#define LegacyAppName "RMD ADE"
#define LegacyAppExe "rmd-ade.exe"
#define LegacyAgentsExe "rmd-ade-agents.exe"

[Setup]
AppId={{6F1B8D2E-5C3A-4E8B-9F27-2B7D1C4A9E51}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
DefaultDirName={autopf}\{#AppName}
; The AppId is the RMD ADE one, so an upgrade would otherwise reuse its folder.
UsePreviousAppDir=no
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#Root}\target\installer
OutputBaseFilename=krad-ade-{#AppVersion}-setup
SetupIconFile={#Root}\packaging\windows\{#AppIcon}
UninstallDisplayIcon={app}\{#AppExe}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[InstallDelete]
Type: files; Name: "{app}\{#LegacyAppExe}"
Type: files; Name: "{app}\{#LegacyAgentsExe}"
Type: files; Name: "{autoprograms}\{#LegacyAppName}.lnk"
Type: files; Name: "{autodesktop}\{#LegacyAppName}.lnk"
Type: filesandordirs; Name: "{autopf}\{#LegacyAppName}"

[UninstallDelete]
Type: filesandordirs; Name: "{autopf}\{#LegacyAppName}"

[Files]
Source: "{#Release}\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Release}\{#AgentsExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\assets\icons\LICENSE-hugeicons.txt"; DestDir: "{app}\licenses"; Flags: ignoreversion
Source: "{#Root}\assets\icons\LICENSE-simple-icons.txt"; DestDir: "{app}\licenses"; Flags: ignoreversion
Source: "{#Root}\assets\fonts\JetBrainsMono-OFL.txt"; DestDir: "{app}\licenses"; Flags: ignoreversion
; Next to the app, portable-pty loads them in place of the system's ConPTY (ADR-0025).
Source: "{#Root}\target\conpty\conpty.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\target\conpty\OpenConsole.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\packaging\windows\LICENSE-conpty.txt"; DestDir: "{app}\licenses"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
