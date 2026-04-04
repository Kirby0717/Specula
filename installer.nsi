!include "MUI2.nsh"
!include "x64.nsh"

!define PRODUCT_NAME "specula"
!ifndef PRODUCT_VERSION
  !define PRODUCT_VERSION "dev"
!endif

Name "${PRODUCT_NAME} ${PRODUCT_VERSION}"
OutFile "${PRODUCT_NAME}-${PRODUCT_VERSION}-setup.exe"
InstallDir "$PROGRAMFILES\${PRODUCT_NAME}"

RequestExecutionLevel admin

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "LICENSE-AGREEMENT.txt"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "Japanese"

Function .onInit
  ${If} ${RunningX64}
    StrCpy $INSTDIR "$PROGRAMFILES64\${PRODUCT_NAME}"
    SetRegView 64
    ${DisableX64FSRedirection}
  ${Else}
    MessageBox MB_OK|MB_ICONSTOP "このアプリケーションは64bit Windowsが必要です。"
    Abort
  ${EndIf}
FunctionEnd

Section "Install"
  SetOutPath "$INSTDIR"

  File "target\x86_64-pc-windows-gnu\release\specula.exe"
  File "LICENSE-MIT"
  File "LICENSE-APACHE"
  File "THIRD-PARTY-LICENSES.html"

  CreateShortcut "$DESKTOP\specula.lnk" "$INSTDIR\specula.exe"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\specula" "DisplayName" "specula"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\specula" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\specula" "DisplayIcon" '"$INSTDIR\specula.exe"'
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\specula" "Publisher" "Kirby0717"
SectionEnd

Function un.onInit
  ${If} ${RunningX64}
    SetRegView 64
    ${DisableX64FSRedirection}
  ${EndIf}
FunctionEnd

Section "Uninstall"
  Delete "$INSTDIR\specula.exe"
  Delete "$INSTDIR\LICENSE-MIT"
  Delete "$INSTDIR\LICENSE-APACHE"
  Delete "$INSTDIR\THIRD-PARTY-LICENSES.html"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$DESKTOP\specula.lnk"
  RMDir "$INSTDIR"

  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\specula"
SectionEnd
