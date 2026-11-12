Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
; makensis changes working directory to the script directory before processing.
!define ROOT ".."
!define APPNAME "Dropper"
!define VERSION "0.2.0"
Name "드롭퍼 ${VERSION}"
OutFile "${ROOT}/artifacts/Dropper_${VERSION}_x64-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\Dropper"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "Dropper · 영상의 무게를 덜어내다"
!define MUI_ICON "${ROOT}/src-tauri/icons/icon.ico"
!define MUI_UNICON "${ROOT}/src-tauri/icons/icon.ico"
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\dropper.exe"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Korean"

Section "드롭퍼" Main
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File "${ROOT}/src-tauri/target-windows/x86_64-pc-windows-msvc/release/dropper.exe"
  File /oname=THIRD_PARTY.md "${ROOT}/THIRD_PARTY.md"
  SetOutPath "$INSTDIR\media"
  File /r "${ROOT}/src-tauri/resources/media-windows/*"
  ; Official detection keys: learn.microsoft.com/microsoft-edge/webview2/concepts/distribution
  SetRegView 32
  ReadRegStr $1 HKLM "SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${If} $1 == ""
  ${OrIf} $1 == "0.0.0.0"
    ReadRegStr $1 HKCU "SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${EndIf}
  ${If} $1 == ""
  ${OrIf} $1 == "0.0.0.0"
    SetOutPath "$TEMP\DropperRuntime"
    File "${ROOT}/artifacts/MicrosoftEdgeWebview2Setup.exe"
    DetailPrint "Microsoft WebView2 런타임 설치 중… (인터넷 연결 필요)"
    ExecWait '"$TEMP\DropperRuntime\MicrosoftEdgeWebview2Setup.exe" /silent /install' $0
    Delete "$TEMP\DropperRuntime\MicrosoftEdgeWebview2Setup.exe"
    RMDir "$TEMP\DropperRuntime"
    ${If} $0 != 0
      MessageBox MB_OK|MB_ICONINFORMATION "WebView2 설치를 완료하지 못했습니다. 드롭퍼가 열리지 않으면 인터넷 연결 후 Microsoft WebView2 Runtime을 설치해 주세요. 오류: $0"
    ${EndIf}
  ${EndIf}
  SetOutPath "$INSTDIR"
  CreateDirectory "$SMPROGRAMS\Dropper"
  CreateShortCut "$SMPROGRAMS\Dropper\드롭퍼.lnk" "$INSTDIR\dropper.exe"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Dropper" "DisplayName" "드롭퍼"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Dropper" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Dropper" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Dropper" "InstallLocation" "$INSTDIR"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Dropper" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Dropper" "NoRepair" 1
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  Delete "$INSTDIR\dropper.exe"
  Delete "$INSTDIR\THIRD_PARTY.md"
  Delete "$INSTDIR\media\bin\ffmpeg.exe"
  Delete "$INSTDIR\media\bin\ffprobe.exe"
  Delete "$INSTDIR\media\licenses\LICENSE.txt"
  Delete "$INSTDIR\media\licenses\source.json"
  Delete "$INSTDIR\media\manifest.json"
  Delete "$INSTDIR\media\.dropper-media"
  RMDir "$INSTDIR\media\bin"
  RMDir "$INSTDIR\media\licenses"
  RMDir "$INSTDIR\media"
  Delete "$SMPROGRAMS\Dropper\드롭퍼.lnk"
  RMDir "$SMPROGRAMS\Dropper"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Dropper"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
