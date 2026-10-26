; OxyPixel per-user installer.
; Unsigned. See docs/BUILD-WINDOWS.md before distributing the output.

Unicode true
ManifestDPIAware true

!define APPNAME "OxyPixel"
!define VERSION "0.7.0"
!define PUBLISHER "OxyPixel contributors"
!define REGPATH "Software\Microsoft\Windows\CurrentVersion\Uninstall\OxyPixel"

; SOURCE_EXE and OUTFILE may be absolute paths. The File instruction does not
; look in ../dist when makensis is started from the repository root.
!ifndef SOURCE_EXE
  !define SOURCE_EXE "../dist/oxypixel.exe"
!endif
!ifndef OUTFILE
  !define OUTFILE "../dist/${APPNAME}-${VERSION}-Setup.exe"
!endif

Name "${APPNAME}"
Caption "${APPNAME} ${VERSION} Setup"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\OxyPixel"
InstallDirRegKey HKCU "${REGPATH}" "InstallLocation"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "${APPNAME} ${VERSION}"

!include "MUI2.nsh"

!define MUI_ABORTWARNING
!define MUI_ICON "oxypixel.ico"
!define MUI_UNICON "oxypixel.ico"

!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_RIGHT
!define MUI_HEADERIMAGE_BITMAP "header.bmp"
!define MUI_HEADERIMAGE_UNBITMAP "header.bmp"
!define MUI_WELCOMEFINISHPAGE_BITMAP "welcome.bmp"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP "welcome.bmp"
!define MUI_WELCOMEFINISHPAGE_BITMAP_NOSTRETCH
!define MUI_UNWELCOMEFINISHPAGE_BITMAP_NOSTRETCH

!define MUI_WELCOMEPAGE_TITLE "Welcome to OxyPixel"
!define MUI_WELCOMEPAGE_TEXT "Setup installs OxyPixel ${VERSION} for the current user.$\r$\n$\r$\nIt adds a Start menu shortcut and opens .oxypix files with OxyPixel. A desktop shortcut is optional."

!define MUI_LICENSEPAGE_TEXT_TOP "OxyPixel is released under the MIT License."
!define MUI_LICENSEPAGE_TEXT_BOTTOM "If you accept the terms, choose I Agree to continue."

!define MUI_COMPONENTSPAGE_SMALLDESC
!define MUI_COMPONENTSPAGE_TEXT_TOP "The editor is always installed. A desktop shortcut is optional."

!define MUI_DIRECTORYPAGE_TEXT_TOP "Choose a folder. This install is for the current user and does not need administrator rights."
!define MUI_DIRECTORYPAGE_TEXT_DESTINATION "Install OxyPixel in:"

!define MUI_FINISHPAGE_TITLE "OxyPixel is installed"
!define MUI_FINISHPAGE_TEXT "OxyPixel ${VERSION} is ready.$\r$\n$\r$\nOpen it from the Start menu, or launch it now. This build is unsigned, so Windows SmartScreen may say the publisher is unknown."
!define MUI_FINISHPAGE_TEXT_LARGE
!define MUI_FINISHPAGE_RUN "$INSTDIR\oxypixel.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch OxyPixel"
!define MUI_FINISHPAGE_NOREBOOTSUPPORT

!define MUI_UNCONFIRMPAGE_TEXT_TOP "This removes OxyPixel for the current user, including the Start menu shortcut and the .oxypix file association."
!define MUI_UNCONFIRMPAGE_TEXT_LOCATION "Installed in:"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "../LICENSE"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

VIProductVersion "0.7.0.0"
VIFileVersion "0.7.0.0"
VIAddVersionKey "ProductName" "OxyPixel"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "FileDescription" "OxyPixel installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "InternalName" "OxyPixel Setup"
VIAddVersionKey "OriginalFilename" "${APPNAME}-${VERSION}-Setup.exe"
VIAddVersionKey "LegalCopyright" "Copyright (C) 2026 OxyPixel contributors. MIT License."
VIAddVersionKey "Comments" "Unsigned build. No Authenticode certificate was applied."

Section "OxyPixel" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"
  File "${SOURCE_EXE}"
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
  WriteRegStr HKCU "${REGPATH}" "QuietUninstallString" "$\"$INSTDIR\Uninstall.exe$\" /S"
  WriteRegStr HKCU "${REGPATH}" "DisplayIcon" "$INSTDIR\oxypixel.ico"
  WriteRegDWORD HKCU "${REGPATH}" "NoModify" 1
  WriteRegDWORD HKCU "${REGPATH}" "NoRepair" 1
  SectionGetSize ${SecMain} $0
  WriteRegDWORD HKCU "${REGPATH}" "EstimatedSize" $0

  WriteRegStr HKCU "Software\Classes\.oxypix" "" "OxyPixel.Document"
  WriteRegStr HKCU "Software\Classes\OxyPixel.Document" "" "OxyPixel document"
  WriteRegStr HKCU "Software\Classes\OxyPixel.Document\DefaultIcon" "" "$INSTDIR\oxypixel.ico"
  WriteRegStr HKCU "Software\Classes\OxyPixel.Document\shell\open\command" "" "$\"$INSTDIR\oxypixel.exe$\" $\"%1$\""
SectionEnd

Section /o "Desktop shortcut" SecDesktop
  CreateShortcut "$DESKTOP\OxyPixel.lnk" "$INSTDIR\oxypixel.exe" "" "$INSTDIR\oxypixel.ico" 0
SectionEnd

LangString DESC_MAIN ${LANG_ENGLISH} "The editor, a Start menu shortcut, and the .oxypix file association."
LangString DESC_DESKTOP ${LANG_ENGLISH} "Also place a shortcut on the desktop."

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
