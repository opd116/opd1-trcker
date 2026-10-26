; Installer hooks for OPD1 TRCKER.
; Included by Tauri's NSIS template (installer.nsi) via
; `bundle.windows.nsis.installerHooks`. Available defines come from that
; template: ${MAINBINARYNAME}, ${BUNDLEID}, ${PRODUCTNAME}, etc.
;
; Goal: a reinstall/upgrade leaves no stale program files, no stale image
; cache, and no stale settings behind -- but NEVER touches the SQLite DB at
; $APPDATA\${BUNDLEID}\data\opd1.sqlite, which is the app's only persistent
; memory and must survive every reinstall.

!macro NSIS_HOOK_PREINSTALL
  ; --- A running copy first ---
  ; The app usually sits in the tray (close to tray and launch with Windows
  ; are on by default). The template asks to close it only after this hook,
  ; so the clean-up below would run against locked files, and a Cancel at
  ; that prompt would leave the old install half deleted (no uninstaller).
  ; Asking here first makes a Cancel leave everything as it was.
  !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"

  ; --- Program files ($INSTDIR) ---
  ; Only when $INSTDIR provably holds a previous install of this app (our
  ; main binary is there), and recursively only when the folder is the app's
  ; own (named after the product, the default). A folder the installer was
  ; pointed at by hand (say D:\Games) loses only the files this app puts
  ; there (Section Install: the exe, THIRD-PARTY-NOTICES.md, OFL.txt,
  ; uninstall.exe);
  ; anything else in it stays (R4).
  ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    Push $0
    ${GetFileName} "$INSTDIR" $0
    ${If} $0 == "${PRODUCTNAME}"
      RMDir /r "$INSTDIR"
      ; Section Install already called SetOutPath "$INSTDIR" before this hook
      ; runs and is about to `File` the new binary into it, so recreate the
      ; directory we just removed.
      CreateDirectory "$INSTDIR"
    ${Else}
      Delete "$INSTDIR\${MAINBINARYNAME}.exe"
      Delete "$INSTDIR\THIRD-PARTY-NOTICES.md"
      Delete "$INSTDIR\OFL.txt"
      Delete "$INSTDIR\uninstall.exe"
    ${EndIf}
    Pop $0
  ${EndIf}

  ; --- Image / WebView2 cache ($LOCALAPPDATA\${BUNDLEID}) ---
  ; Entirely disposable: re-downloaded lazily on next run.
  RMDir /r "$LOCALAPPDATA\${BUNDLEID}"

  ; --- Settings ($APPDATA\${BUNDLEID}) ---
  ; Delete everything except the "data" folder, which holds the SQLite DB
  ; (opd1.sqlite). The DB is the app's memory and must survive reinstalls.
  ${If} ${FileExists} "$APPDATA\${BUNDLEID}"
    Push $0
    Push $1
    FindFirst $0 $1 "$APPDATA\${BUNDLEID}\*"
    appdata_clean_loop:
      StrCmp $1 "" appdata_clean_done
      StrCmp $1 "." appdata_clean_next
      StrCmp $1 ".." appdata_clean_next
      StrCmp $1 "data" appdata_clean_next
      ${If} ${FileExists} "$APPDATA\${BUNDLEID}\$1\*.*"
        RMDir /r "$APPDATA\${BUNDLEID}\$1"
      ${Else}
        Delete "$APPDATA\${BUNDLEID}\$1"
      ${EndIf}
      appdata_clean_next:
        FindNext $0 $1
        Goto appdata_clean_loop
    appdata_clean_done:
      FindClose $0
    Pop $1
    Pop $0
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; --- Launch with Windows (R189, R194) ---
  ; The app's "Launch with Windows" setting writes a Run value named after
  ; the product ("OPD1 TRCKER", src-tauri/src/v2/window.rs). Remove it so an
  ; uninstalled app never starts a missing exe. Task Manager's
  ; StartupApproved entry is left alone: it is the user's own on/off switch,
  ; Windows ignores it without a Run value, and an upgrade (which runs this
  ; uninstaller first, without /UPDATE) must not turn a switched-off app back
  ; on. The new app writes only the Run value at its first start.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "OPD1 TRCKER"
!macroend
