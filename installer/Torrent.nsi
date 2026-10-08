Unicode true
!include "nsDialogs.nsh"
!include "LogicLib.nsh"
!include "WinMessages.nsh"
!include "FileFunc.nsh"
Name "Torrent"
OutFile "..\dist\Torrent-Payload.exe"
InstallDir "$LOCALAPPDATA\Programs\Torrent"
RequestExecutionLevel user
SetCompressor /SOLID lzma
Icon "..\assets\app.ico"
UninstallIcon "..\assets\app.ico"
BrandingText " "
ShowInstDetails nevershow
ShowUninstDetails nevershow
Page custom Welcome
Page instfiles "" Progress
Page custom Finished FinishLeave
UninstPage instfiles
Var Dialog
Var Check
Var Launch
Var IconHandle
Var Label
Var Font
Var ProgressFile
Var ProgressValue
Function .onInit
 ${GetParameters} $0
 ${GetOptions} $0 "/PROGRESSFILE=" $ProgressFile
 StrCpy $Launch ${BST_CHECKED}
 InitPluginsDir
 File /oname=$PLUGINSDIR\app.ico "..\assets\app.ico"
FunctionEnd
Function Header
 System::Call 'user32::SetWindowPos(p $Dialog, p 0, i 18, i 20, i 500, i 570, i 0x14)'
 GetDlgItem $0 $HWNDPARENT 1
 ShowWindow $0 ${SW_HIDE}
 GetDlgItem $0 $HWNDPARENT 2
 ShowWindow $0 ${SW_HIDE}
 GetDlgItem $0 $HWNDPARENT 3
 ShowWindow $0 ${SW_HIDE}
 SetCtlColors $Dialog 172033 F8FAFD
 ${NSD_CreateIcon} 103u 8u 96u 96u ""
 Pop $0
 ${NSD_SetIcon} $0 "$PLUGINSDIR\app.ico" $IconHandle
FunctionEnd
Function .onGUIInit
 SetAutoClose true
 System::Call 'user32::SetWindowPos(p $HWNDPARENT, p 0, i 0, i 0, i 540, i 640, i 0x16)'
FunctionEnd
Function Welcome
 nsDialogs::Create 1018
 Pop $Dialog
 Call Header
 ${NSD_CreateLabel} 0u 104u 300u 30u "Torrent"
 Pop $Label
 CreateFont $Font "Segoe UI" 24 600
 SendMessage $Label ${WM_SETFONT} $Font 1
 SendMessage $Label ${STM_SETIMAGE} 0 0
 ${NSD_AddStyle} $Label ${SS_CENTER}
 ${NSD_CreateLabel} 0u 141u 300u 24u "Простой торрент-клиент для Windows"
 Pop $0
 ${NSD_AddStyle} $0 ${SS_CENTER}
 ${NSD_CreateButton} 34u 174u 232u 30u "Установить  →"
 Pop $0
 ${NSD_OnClick} $0 InstallClick
 nsDialogs::Show
FunctionEnd
Function InstallClick
 SendMessage $HWNDPARENT ${WM_COMMAND} 1 0
FunctionEnd
Function Progress
 GetDlgItem $0 $HWNDPARENT 1
 ShowWindow $0 ${SW_HIDE}
 GetDlgItem $0 $HWNDPARENT 2
 ShowWindow $0 ${SW_HIDE}
 GetDlgItem $0 $HWNDPARENT 3
 ShowWindow $0 ${SW_HIDE}
 GetDlgItem $0 $HWNDPARENT 1016
 ShowWindow $0 ${SW_HIDE}
 FindWindow $Dialog "#32770" "" $HWNDPARENT
 System::Call 'user32::SetWindowPos(p $Dialog, p 0, i 18, i 20, i 500, i 570, i 0x14)'
 System::Call 'user32::CreateWindowExW(i 0, w "STATIC", w "", i 0x50000003, i 162, i 32, i 160, i 160, p $Dialog, p 0, p 0, p 0) p .r0'
 System::Call 'user32::LoadImageW(p 0, w "$PLUGINSDIR\app.ico", i 1, i 160, i 160, i 0x10) p .r1'
 SendMessage $0 ${STM_SETIMAGE} 1 $1
 System::Call 'user32::CreateWindowExW(i 0, w "STATIC", w "Установка", i 0x50000001, i 20, i 230, i 440, i 48, p $Dialog, p 0, p 0, p 0) p .r0'
 SendMessage $0 ${WM_SETFONT} $Font 1
 System::Call 'user32::CreateWindowExW(i 0, w "STATIC", w "Копирование файлов…", i 0x50000001, i 20, i 290, i 440, i 32, p $Dialog, p 0, p 0, p 0) p .r0'
 GetDlgItem $0 $Dialog 1004
 System::Call 'user32::SetWindowPos(p r0, p 0, i 42, i 352, i 400, i 16, i 0x14)'
 GetDlgItem $0 $Dialog 1016
 ShowWindow $0 ${SW_HIDE}
 GetDlgItem $0 $Dialog 1006
 ShowWindow $0 ${SW_HIDE}
 GetDlgItem $0 $Dialog 1027
 ShowWindow $0 ${SW_HIDE}
FunctionEnd
Function Finished
 nsDialogs::Create 1018
 Pop $Dialog
 Call Header
 ${NSD_CreateLabel} 0u 106u 300u 32u "Готово"
 Pop $Label
 SendMessage $Label ${WM_SETFONT} $Font 1
 ${NSD_AddStyle} $Label ${SS_CENTER}
 ${NSD_CreateLabel} 0u 142u 300u 24u "Приложение успешно установлено"
 Pop $0
 ${NSD_AddStyle} $0 ${SS_CENTER}
 ${NSD_CreateCheckbox} 30u 172u 270u 18u "Запустить Torrent после установки"
 Pop $Check
 ${NSD_SetState} $Check $Launch
 ${NSD_CreateButton} 34u 204u 232u 30u "Готово"
 Pop $0
 ${NSD_OnClick} $0 FinishClick
 nsDialogs::Show
FunctionEnd
Function FinishClick
 ${NSD_GetState} $Check $Launch
 SendMessage $HWNDPARENT ${WM_COMMAND} 1 0
FunctionEnd
Function FinishLeave
 ${If} $Launch == ${BST_CHECKED}
  Exec '"$INSTDIR\Torrent.exe"'
 ${EndIf}
FunctionEnd
Function ReportProgress
 ${If} $ProgressFile != ""
  FileOpen $9 $ProgressFile w
  FileWrite $9 "$ProgressValue"
  FileClose $9
 ${EndIf}
FunctionEnd
Section
 StrCpy $ProgressValue 10
 Call ReportProgress
 SetDetailsPrint none
 SetOutPath "$INSTDIR"
 File /r "..\dist\app\*.*"
 StrCpy $ProgressValue 85
 Call ReportProgress
 WriteUninstaller "$INSTDIR\Uninstall.exe"
 CreateDirectory "$SMPROGRAMS\Torrent"
 CreateShortcut "$SMPROGRAMS\Torrent\Torrent.lnk" "$INSTDIR\Torrent.exe"
 WriteRegStr HKCU "Software\Classes\Torrent.File" "" "Torrent file"
 WriteRegStr HKCU "Software\Classes\Torrent.File\DefaultIcon" "" '"$INSTDIR\Torrent.exe",0'
 WriteRegStr HKCU "Software\Classes\Torrent.File\shell\open\command" "" '"$INSTDIR\Torrent.exe" "%1"'
 WriteRegStr HKCU "Software\Classes\.torrent\OpenWithProgids" "Torrent.File" ""
 WriteRegStr HKCU "Software\Classes\Torrent.Magnet" "" "URL:Torrent magnet"
 WriteRegStr HKCU "Software\Classes\Torrent.Magnet" "URL Protocol" ""
 WriteRegStr HKCU "Software\Classes\Torrent.Magnet\shell\open\command" "" '"$INSTDIR\Torrent.exe" "%1"'
 WriteRegStr HKCU "Software\Torrent\Capabilities" "ApplicationName" "Torrent"
 WriteRegStr HKCU "Software\Torrent\Capabilities\FileAssociations" ".torrent" "Torrent.File"
 WriteRegStr HKCU "Software\Torrent\Capabilities\URLAssociations" "magnet" "Torrent.Magnet"
 WriteRegStr HKCU "Software\RegisteredApplications" "Torrent" "Software\Torrent\Capabilities"
 WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Torrent" "DisplayName" "Torrent"
 WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Torrent" "DisplayVersion" "0.1.4"
 WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Torrent" "UninstallString" '"$INSTDIR\Uninstall.exe"'
 WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Torrent" "DisplayIcon" "$INSTDIR\Torrent.exe"
 System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
 StrCpy $ProgressValue 100
 Call ReportProgress
SectionEnd
Section "Uninstall"
 DeleteRegKey HKCU "Software\Classes\Torrent.File"
 DeleteRegKey HKCU "Software\Classes\Torrent.Magnet"
 DeleteRegValue HKCU "Software\Classes\.torrent\OpenWithProgids" "Torrent.File"
 DeleteRegValue HKCU "Software\RegisteredApplications" "Torrent"
 DeleteRegKey HKCU "Software\Torrent\Capabilities"
 DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Torrent"
 Delete "$SMPROGRAMS\Torrent\Torrent.lnk"
 RMDir "$SMPROGRAMS\Torrent"
 Delete "$INSTDIR\Torrent.exe"
 Delete "$INSTDIR\Uninstall.exe"
 RMDir /r "$INSTDIR\licenses"
 RMDir "$INSTDIR"
 System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
SectionEnd
