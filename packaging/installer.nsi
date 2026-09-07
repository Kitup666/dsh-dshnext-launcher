; Dshnext NSIS installer - per-user, no admin, no runtime prerequisites.
; The whole point of the native rewrite is a single green exe; this installer
; only adds a Start-menu entry + uninstaller for people who want one.
; Build:  makensis installer.nsi   (keep this file pure ASCII to dodge codepage issues)
;
; NOTE: identity is deliberately "Dshnext", NOT "DshDesk" -- the first-gen Tauri
; app registers under Uninstall\DshDesk. Reusing that key (and an InstallDirRegKey
; pointing at it) made this installer read the first-gen's stale InstallLocation and
; drop its exe into the wrong folder. Keep the two products' registry identities apart.

!define APP_NAME    "DshDesk Native"
!define APP_ID      "Dshnext"
!define APP_EXE     "dshnext.exe"
!define APP_VERSION "0.1.9"
!define UNINST_KEY  "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}"

Unicode true
SetCompressor /SOLID lzma
Name "${APP_NAME}"
OutFile "${APP_ID}_${APP_VERSION}_x64-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\${APP_ID}"
RequestExecutionLevel user   ; per-user, no UAC prompt

!include "MUI2.nsh"
!define MUI_ABORTWARNING
!define MUI_ICON "..\assets\icons\app.ico"
!define MUI_UNICON "..\assets\icons\app.ico"

!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "Install"
  SetOutPath "$INSTDIR"
  File "..\target\release\${APP_EXE}"
  CreateDirectory "$SMPROGRAMS\${APP_ID}"
  CreateShortCut "$SMPROGRAMS\${APP_ID}\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}"
  CreateShortCut "$SMPROGRAMS\${APP_ID}\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
  ; Add/Remove Programs entry
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\${APP_EXE}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "DshDesk"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" "$INSTDIR\Uninstall.exe"
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" 21790   ; KB, = release exe size
  WriteUninstaller "$INSTDIR\Uninstall.exe"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir  "$INSTDIR"
  ; The two shortcuts must be deleted by name: plain RMDir refuses a non-empty
  ; folder, so without these the Start-menu entry survives every uninstall.
  Delete "$SMPROGRAMS\${APP_ID}\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${APP_ID}\Uninstall.lnk"
  RMDir  "$SMPROGRAMS\${APP_ID}"
  DeleteRegKey HKCU "${UNINST_KEY}"
SectionEnd
