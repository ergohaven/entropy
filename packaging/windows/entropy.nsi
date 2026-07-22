; NSIS installer for Entropy, built from Linux with makensis:
;   makensis -DVERSION=$VERSION -DEXE_SOURCE=path/to/entropy.exe \
;            -DOUTFILE=entropy-setup-$VERSION.exe packaging/windows/entropy.nsi

Unicode true

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef EXE_SOURCE
  !define EXE_SOURCE "target/x86_64-pc-windows-gnu/release/entropy.exe"
!endif
!ifndef OUTFILE
  !define OUTFILE "dist/windows/entropy-setup-${VERSION}.exe"
!endif
; makensis resolves relative paths unreliably here; the Taskfile passes ICON,
; LICENSE_FILE and EXE_SOURCE as absolute paths.
!ifndef ICON
  !define ICON "assets/entropy.ico"
!endif
!ifndef LICENSE_FILE
  !define LICENSE_FILE "LICENSE"
!endif

!define APP_NAME "Entropy"
!define COMPANY "Ergohaven"
!define APP_URL "https://github.com/ergohaven/entropy"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Entropy"

!include "MUI2.nsh"

Name "${APP_NAME}"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES64\${COMPANY}\${APP_NAME}"
InstallDirRegKey HKLM "Software\${COMPANY}\${APP_NAME}" "InstallDir"
RequestExecutionLevel admin

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${APP_NAME}"
VIAddVersionKey "FileDescription" "${APP_NAME}"
VIAddVersionKey "CompanyName" "${COMPANY}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "© ${COMPANY}"

!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${LICENSE_FILE}"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\entropy.exe"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "Russian"

Section "Entropy" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"
  File "/oname=entropy.exe" "${EXE_SOURCE}"

  CreateDirectory "$SMPROGRAMS\${APP_NAME}"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk" "$INSTDIR\entropy.exe"
  CreateShortcut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\entropy.exe"

  WriteRegStr HKLM "Software\${COMPANY}\${APP_NAME}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNINST_KEY}" "Publisher" "${COMPANY}"
  WriteRegStr HKLM "${UNINST_KEY}" "URLInfoAbout" "${APP_URL}"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\entropy.exe"
  WriteRegStr HKLM "${UNINST_KEY}" "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegDWORD HKLM "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINST_KEY}" "NoRepair" 1

  WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\entropy.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  RMDir "$PROGRAMFILES64\${COMPANY}"

  Delete "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk"
  RMDir "$SMPROGRAMS\${APP_NAME}"
  Delete "$DESKTOP\${APP_NAME}.lnk"

  DeleteRegKey HKLM "${UNINST_KEY}"
  DeleteRegKey HKLM "Software\${COMPANY}\${APP_NAME}"
SectionEnd
