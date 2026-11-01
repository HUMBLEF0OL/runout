; Runout's source-A cleanup belongs in the NSIS pre-uninstall hook.
; Tauri's uninstaller sets $UpdateMode from /UPDATE before inserting this hook.
; The template runs this hook *before* its own CheckIfAppIsRunning, so close the app here first: a running
; Runout holds spotify_cache.db and its WebView2 processes hold EBWebView, and the cleanup can't delete them.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
    Sleep 1000 ; WebView2 child processes exit just after the host
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall-cleanup' $0
    ${If} $0 <> 0
      DetailPrint "Runout cleanup failed (exit $0): cache or WebView data may remain"
    ${EndIf}
  ${EndIf}
!macroend
