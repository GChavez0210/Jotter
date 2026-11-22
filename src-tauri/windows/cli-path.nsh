; Puts the install folder on the current user's PATH so `jotter` runs from any terminal, and
; takes it off again on uninstall. The per-user installer writes HKCU only.
;
; PowerShell edits the value because NSIS strings stop at 1024 characters, and a longer PATH read
; through ReadRegStr would come back truncated. The script reads the raw REG_EXPAND_SZ value so
; entries such as %USERPROFILE%\bin stay unexpanded, and it writes only when the value changes,
; so the reinstall that each update runs leaves PATH alone.

!macro JOTTER_EDIT_USER_PATH ACTION
  System::Call 'Kernel32::SetEnvironmentVariable(t "JOTTER_CLI_DIR", t "$INSTDIR")'
  System::Call 'Kernel32::SetEnvironmentVariable(t "JOTTER_PATH_ACTION", t "${ACTION}")'
  nsExec::ExecToLog `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$$d = $$env:JOTTER_CLI_DIR.TrimEnd('\'); $$k = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment'); $$p = [string]$$k.GetValue('Path', '', 'DoNotExpandEnvironmentNames'); $$keep = @($$p -split ';' | Where-Object { $$_ -and $$_.TrimEnd('\') -ne $$d }); if ($$env:JOTTER_PATH_ACTION -eq 'add') { $$keep += $$d }; $$new = $$keep -join ';'; if ($$new -ne $$p) { $$k.SetValue('Path', $$new, 'ExpandString') }"`
  Pop $0
  ; Tell open programs, such as Explorer, to reload the environment. 0xFFFF is HWND_BROADCAST
  ; and 0x1A is WM_SETTINGCHANGE.
  SendMessage 0xFFFF 0x1A 0 "STR:Environment" /TIMEOUT=5000
!macroend

; MCP clients keep `jotter mcp` running, and Windows refuses to overwrite a running executable,
; so an update would fail to replace jotter.exe. Windows does allow renaming it. Move a locked
; copy aside so the new one can be written. Each update uses a free backup name because
; different MCP clients can keep several versions running at once.
!macro NSIS_HOOK_PREINSTALL
  Delete "$INSTDIR\jotter.exe.old"
  Delete "$INSTDIR\jotter.exe.old.*"
  Delete "$INSTDIR\jotter.exe"
  ${If} ${FileExists} "$INSTDIR\jotter.exe"
    StrCpy $0 0
    ${DoWhile} ${FileExists} "$INSTDIR\jotter.exe.old.$0"
      IntOp $0 $0 + 1
    ${Loop}
    ClearErrors
    Rename "$INSTDIR\jotter.exe" "$INSTDIR\jotter.exe.old.$0"
    ${If} ${Errors}
      MessageBox MB_OK|MB_ICONSTOP "Cannot replace the Jotter command-line tool. Close your MCP clients and try again."
      Abort
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro JOTTER_EDIT_USER_PATH "add"
!macroend

; Per-user installs cannot rely on scheduling locked files for deletion at reboot, which
; needs administrator access. Stop before removing the app or its PATH entry instead.
!macro NSIS_HOOK_PREUNINSTALL
  Delete "$INSTDIR\jotter.exe.old"
  Delete "$INSTDIR\jotter.exe.old.*"
  ${If} ${FileExists} "$INSTDIR\jotter.exe.old"
  ${OrIf} ${FileExists} "$INSTDIR\jotter.exe.old.*"
    MessageBox MB_OK|MB_ICONSTOP "Cannot remove the Jotter command-line tool. Close your MCP clients and try uninstalling again."
    Abort
  ${EndIf}
  Delete "$INSTDIR\jotter.exe"
  ${If} ${FileExists} "$INSTDIR\jotter.exe"
    MessageBox MB_OK|MB_ICONSTOP "Cannot remove the Jotter command-line tool. Close your MCP clients and try uninstalling again."
    Abort
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  !insertmacro JOTTER_EDIT_USER_PATH "remove"
!macroend
