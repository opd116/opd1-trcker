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
  ; --- Program files ($INSTDIR) ---
  ; Only wipe $INSTDIR when it provably already holds a previous install of
  ; this app (our main binary is present there). Never recursively delete a
  ; directory just because the installer happened to be pointed at it -- an
  ; empty/unrelated $INSTDIR is left completely alone.
  ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    RMDir /r "$INSTDIR"
    ; Section Install already called SetOutPath "$INSTDIR" before this hook
    ; runs and is about to `File` the new binary into it, so recreate the
    ; directory we just removed.
    CreateDirectory "$INSTDIR"
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
