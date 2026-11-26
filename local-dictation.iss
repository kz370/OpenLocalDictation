; Open Local Dictation - Inno Setup script (all users, C:\Program Files)
; Built by scripts\build.bat, which passes /DAppVersion /DSourceExe /DLibDir /DOutputDir.

#define AppName "Open Local Dictation"

[Setup]
AppId={{7A3E1C52-9B04-4D6F-8E2A-5C71D0B9F3A4}}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=Open Local Dictation
DefaultDirName={autopf}\Open Local Dictation
PrivilegesRequired=admin
OutputDir={#OutputDir}
OutputBaseFilename=Open-Local-Dictation-{#AppVersion}-setup
SetupIconFile=src-tauri\icons\icon.ico
UninstallDisplayIcon={app}\Open Local Dictation.exe
Compression=lzma2/max
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "Open Local Dictation.exe"; Flags: ignoreversion
; Speech runs on sherpa-onnx / ONNX Runtime, linked dynamically so the optional
; CUDA pack can replace them at launch.
Source: "{#LibDir}\sherpa-onnx-c-api.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#LibDir}\sherpa-onnx-cxx-api.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#LibDir}\onnxruntime.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#LibDir}\onnxruntime_providers_shared.dll"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Open Local Dictation"; Filename: "{app}\Open Local Dictation.exe"
Name: "{autodesktop}\Open Local Dictation"; Filename: "{app}\Open Local Dictation.exe"; Tasks: desktopicon

[Tasks]
Name: desktopicon; Description: "Create a &desktop icon"; Flags: unchecked

[Run]
Filename: "{app}\Open Local Dictation.exe"; Description: "Launch Open Local Dictation"; Flags: nowait postinstall skipifsilent
