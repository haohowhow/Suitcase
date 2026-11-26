!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    koofr_cleanup_retry:
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --unregister-sync-roots' $0
    ${If} $0 <> 0
      MessageBox MB_RETRYCANCEL|MB_ICONSTOP "Suitcase 无法解除 Windows 按需同步根。请完全退出 Suitcase 后重试；取消将保留当前安装，避免留下无主同步根。" IDRETRY koofr_cleanup_retry
      Abort
    ${EndIf}
  ${EndIf}
!macroend
