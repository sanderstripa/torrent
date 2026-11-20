Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"

!ifndef STAGE
  !error "Pass /DSTAGE=<prepared payload folder>"
!endif
!ifndef OUTPUT
  !error "Pass /DOUTPUT=<installer executable path>"
!endif

Name "Torrent 1.4.0 Rust Edition Preview 2"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\Programs\Torrent Rust Edition"
RequestExecutionLevel user
SetCompressor /SOLID lzma
Icon "..\..\assets\app.ico"
UninstallIcon "..\..\assets\app.ico"
VIProductVersion "1.4.0.2"
VIAddVersionKey /LANG=1033 "ProductName" "Torrent Rust Edition"
VIAddVersionKey /LANG=1033 "FileDescription" "Torrent Rust Edition Preview 2 Setup"
VIAddVersionKey /LANG=1033 "FileVersion" "1.4.0 Preview 2"
VIAddVersionKey /LANG=1033 "ProductVersion" "1.4.0 Preview 2"
VIAddVersionKey /LANG=1033 "LegalCopyright" "Sander Stripa"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "Russian"

Section "Torrent Rust Edition"
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File "${STAGE}\Torrent-RE.exe"
  File "${STAGE}\torrent_re_libtorrent.dll"
  File /r "${STAGE}\licenses"
  File /oname=Torrent-RE.ico "..\..\assets\app.ico"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateShortcut "$SMPROGRAMS\Torrent Rust Edition.lnk" "$INSTDIR\Torrent-RE.exe" "" "$INSTDIR\Torrent-RE.ico"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TorrentRustEdition" "DisplayName" "Torrent Rust Edition"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TorrentRustEdition" "DisplayVersion" "1.4.0 Preview 2"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TorrentRustEdition" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegStr HKCU "Software\Classes\TorrentRustEdition.File" "" "Torrent File (Rust Edition)"
  WriteRegStr HKCU "Software\Classes\TorrentRustEdition.File\DefaultIcon" "" "$INSTDIR\Torrent-RE.ico"
  WriteRegStr HKCU "Software\Classes\TorrentRustEdition.File\shell\open\command" "" '$\"$INSTDIR\Torrent-RE.exe$\" $\"%1$\"'
  WriteRegStr HKCU "Software\Classes\.torrent\OpenWithProgids" "TorrentRustEdition.File" ""
  WriteRegStr HKCU "Software\Classes\TorrentRustEdition.Magnet" "" "Magnet link (Torrent Rust Edition)"
  WriteRegStr HKCU "Software\Classes\TorrentRustEdition.Magnet" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\TorrentRustEdition.Magnet\shell\open\command" "" '$\"$INSTDIR\Torrent-RE.exe$\" $\"%1$\"'
  WriteRegStr HKCU "Software\TorrentRustEdition\Capabilities" "ApplicationName" "Torrent Rust Edition"
  WriteRegStr HKCU "Software\TorrentRustEdition\Capabilities\FileAssociations" ".torrent" "TorrentRustEdition.File"
  WriteRegStr HKCU "Software\TorrentRustEdition\Capabilities\URLAssociations" "magnet" "TorrentRustEdition.Magnet"
  WriteRegStr HKCU "Software\RegisteredApplications" "Torrent Rust Edition" "Software\TorrentRustEdition\Capabilities"
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  Delete "$SMPROGRAMS\Torrent Rust Edition.lnk"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TorrentRustEdition"
  DeleteRegValue HKCU "Software\Classes\.torrent\OpenWithProgids" "TorrentRustEdition.File"
  DeleteRegKey HKCU "Software\Classes\TorrentRustEdition.File"
  DeleteRegKey HKCU "Software\Classes\TorrentRustEdition.Magnet"
  DeleteRegValue HKCU "Software\RegisteredApplications" "Torrent Rust Edition"
  DeleteRegKey HKCU "Software\TorrentRustEdition"
  Delete "$INSTDIR\Torrent-RE.exe"
  Delete "$INSTDIR\torrent_re_libtorrent.dll"
  Delete "$INSTDIR\Torrent-RE.ico"
  Delete "$INSTDIR\licenses\*.*"
  RMDir "$INSTDIR\licenses"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
