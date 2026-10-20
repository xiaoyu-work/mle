#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef AppArch
  #error AppArch is required
#endif

[Setup]
AppId={{980498B6-BDBF-490E-8B83-9F5AD76F5C11}
AppName=MLE
AppVersion={#AppVersion}
AppPublisher=MLE contributors
AppPublisherURL=https://github.com/xiaoyu-work/mle
DefaultDirName={localappdata}\Programs\MLE
PrivilegesRequired=lowest
DisableProgramGroupPage=yes
DisableDirPage=yes
OutputDir=..\dist\installers
OutputBaseFilename=MLE-{#AppVersion}-windows-{#AppArch}-setup
SetupIconFile=..\assets\mle.ico
UninstallDisplayIcon={app}\mle.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
CloseApplicationsFilter=mle.exe
#if AppArch == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
MinVersion=10.0.22000
#elif AppArch == "x64"
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
#else
  #error Unsupported AppArch
#endif

[Files]
Source: "..\dist\MLE\mle.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\MLE\data\dictionary.db"; DestDir: "{app}\data"; Flags: ignoreversion
Source: "..\dist\MLE\licenses\ECDICT.txt"; DestDir: "{app}\licenses"; Flags: ignoreversion
Source: "..\dist\MLE\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\MLE"; Filename: "{app}\mle.exe"

[Run]
Filename: "{app}\mle.exe"; Description: "Launch MLE"; Flags: nowait postinstall skipifsilent
