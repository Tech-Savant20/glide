; Glide installer (Inno Setup 6). Installs for the current user only: no admin
; prompt, into %LOCALAPPDATA%\Programs\Glide.
;
; Build: ISCC.exe /DAppVersion=0.1.0 /DDistDir=..\dist installer\glide.iss
; DistDir must contain glide.exe and glide.ico.

#ifndef AppVersion
  #define AppVersion "0.0.0-dev"
#endif
#ifndef DistDir
  #define DistDir "..\dist"
#endif

[Setup]
AppId={{E2C36CBE-E3EA-4900-AFE0-462E8CCA3FDE}
AppName=Glide
AppVersion={#AppVersion}
AppVerName=Glide {#AppVersion}
AppPublisher=Glide contributors
AppPublisherURL=https://github.com/Tech-Savant20/glide
AppSupportURL=https://github.com/Tech-Savant20/glide/issues
AppUpdatesURL=https://github.com/Tech-Savant20/glide/releases
PrivilegesRequired=lowest
DefaultDirName={autopf}\Glide
DisableProgramGroupPage=yes
DisableDirPage=auto
OutputDir={#DistDir}
OutputBaseFilename=Glide-{#AppVersion}-setup
SetupIconFile={#DistDir}\glide.ico
UninstallDisplayIcon={app}\glide.exe
UninstallDisplayName=Glide
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Windows 10 version 1903
MinVersion=10.0.18362
CloseApplications=force
RestartApplications=no
VersionInfoVersion={#AppVersion}
VersionInfoProductName=Glide
VersionInfoDescription=Glide installer

[Tasks]
Name: "startup"; Description: "Start Glide when I sign in to Windows"

[Files]
Source: "{#DistDir}\glide.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Glide"; Filename: "{app}\glide.exe"; Comment: "Smooth scrolling for your mouse wheel"
Name: "{autoprograms}\Glide Settings"; Filename: "{app}\glide.exe"; Parameters: "--settings"; Comment: "Change how Glide scrolls"

[Registry]
; Same value the app's own "Start with Windows" switch writes.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Glide"; ValueData: """{app}\glide.exe"""; Tasks: startup
; Remove it on uninstall even if it was turned on later from inside Glide.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "Glide"; Flags: uninsdeletevalue

[Run]
Filename: "{app}\glide.exe"; Description: "Start Glide now"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/IM glide.exe /F"; Flags: runhidden; RunOnceId: "StopGlide"

[Code]
// An upgrade has to replace glide.exe, so stop the running copy (tray and any
// open settings window) first. Settings in %APPDATA%\Glide are kept.
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ResultCode: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/IM glide.exe /F', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Result := '';
end;
