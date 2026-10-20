; Runout's source-A cleanup belongs in the NSIS pre-uninstall hook.
; Tauri's uninstaller sets $UpdateMode from /UPDATE before inserting this hook.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    ExecWait '"$INSTDIR\runout.exe" --uninstall-cleanup'
  ${EndIf}
!macroend
