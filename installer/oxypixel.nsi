; OxyPixel per-user installer.
; Unsigned. See docs/BUILD-WINDOWS.md before distributing the output.
Unicode true

!define APPNAME "OxyPixel"
!define VERSION "0.1.0"
!define PUBLISHER "OxyPixel contributors"
!define REGPATH "Software\Microsoft\Windows\CurrentVersion\Uninstall\OxyPixel"

Name "${APPNAME} ${VERSION}"
OutFile "../dist/OxyPixel-${VERSION}-Setup.exe"
InstallDir "$LOCALAPPDATA\OxyPixel"
InstallDirRegKey HKCU "${REGPATH}" "InstallLocation"
RequestExecutionLevel user
SetCompressor /SOLID lzma

!include "MUI2.nsh"

!define MUI_ABORTWARNING
!define MUI_ICON "oxypixel.ico"
!define MUI_UNICON "oxypixel.ico"
!define MUI_WELCOMEPAGE_TITLE "Install OxyPixel ${VERSION}"
!define MUI_WELCOMEPAGE_TEXT "This installs OxyPixel for the current user.$\r$\n$\r$\nThis build is unsigned. Windows SmartScreen may say the publisher is unknown because no code-signing certificate was applied. Install it only if you trust the source of this file."

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

VIProductVersion "0.1.0.0"
VIFileVersion "0.1.0.0"
VIAddVersionKey "ProductName" "OxyPixel Setup"
VIAddVersionKey "FileDescription" "OxyPixel installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "OxyPixel contributors"
VIAddVersionKey "Comments" "Unsigned build. No Authenticode certificate was applied."

Section "OxyPixel" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"
  File "../dist/oxypixel.exe"
  File "oxypixel.ico"
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  CreateDirectory "$SMPROGRAMS\OxyPixel"
  CreateShortcut "$SMPROGRAMS\OxyPixel\OxyPixel.lnk" "$INSTDIR\oxypixel.exe" "" "$INSTDIR\oxypixel.ico" 0
  CreateShortcut "$SMPROGRAMS\OxyPixel\Uninstall OxyPixel.lnk" "$INSTDIR\Uninstall.exe"

  WriteRegStr HKCU "${REGPATH}" "DisplayName" "${APPNAME}"
  WriteRegStr HKCU "${REGPATH}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${REGPATH}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKCU "${REGPATH}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${REGPATH}" "UninstallString" "$\"$INSTDIR\Uninstall.exe$\""
  WriteRegStr HKCU "${REGPATH}" "DisplayIcon" "$INSTDIR\oxypixel.ico"
  WriteRegDWORD HKCU "${REGPATH}" "NoModify" 1
  WriteRegDWORD HKCU "${REGPATH}" "NoRepair" 1
  WriteRegStr HKCU "${REGPATH}" "Comments" "Unsigned build. No Authenticode certificate was applied."

  WriteRegStr HKCU "Software\Classes\.oxypix" "" "OxyPixel.Document"
  WriteRegStr HKCU "Software\Classes\OxyPixel.Document" "" "OxyPixel document"
  WriteRegStr HKCU "Software\Classes\OxyPixel.Document\DefaultIcon" "" "$INSTDIR\oxypixel.ico"
  WriteRegStr HKCU "Software\Classes\OxyPixel.Document\shell\open\command" "" "$\"$INSTDIR\oxypixel.exe$\" $\"%1$\""
SectionEnd

Section /o "Desktop shortcut" SecDesktop
  CreateShortcut "$DESKTOP\OxyPixel.lnk" "$INSTDIR\oxypixel.exe" "" "$INSTDIR\oxypixel.ico" 0
SectionEnd

LangString DESC_MAIN ${LANG_ENGLISH} "Program files, Start menu entries, and the .oxypix file association."
LangString DESC_DESKTOP ${LANG_ENGLISH} "Add a desktop shortcut."

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecMain} $(DESC_MAIN)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecDesktop} $(DESC_DESKTOP)
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  Delete "$INSTDIR\oxypixel.exe"
  Delete "$INSTDIR\oxypixel.ico"
  Delete "$INSTDIR\Uninstall.exe"
  Delete "$SMPROGRAMS\OxyPixel\OxyPixel.lnk"
  Delete "$SMPROGRAMS\OxyPixel\Uninstall OxyPixel.lnk"
  RMDir "$SMPROGRAMS\OxyPixel"
  Delete "$DESKTOP\OxyPixel.lnk"
  DeleteRegKey HKCU "${REGPATH}"
  DeleteRegKey HKCU "Software\Classes\.oxypix"
  DeleteRegKey HKCU "Software\Classes\OxyPixel.Document"
  RMDir "$INSTDIR"
SectionEnd
