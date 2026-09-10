; Unibox NSIS lifecycle hooks.
; Native runtime processes keep their executables open on Windows, so an
; in-place upgrade must stop both the desktop process and any orphaned
; child runtime/connector processes before Tauri copies new resources.

!macro UNIBOX_STOP_NATIVE_PROCESSES
  DetailPrint "Stopping Unibox native runtime processes before update..."

  nsExec::ExecToStack '"$SYSDIR\taskkill.exe" /F /T /IM unibox-desktop.exe'
  Pop $0
  Pop $1

  nsExec::ExecToStack '"$SYSDIR\taskkill.exe" /F /T /IM tuwunel.exe'
  Pop $0
  Pop $1

  nsExec::ExecToStack '"$SYSDIR\taskkill.exe" /F /T /IM mautrix-whatsapp.exe'
  Pop $0
  Pop $1

  nsExec::ExecToStack '"$SYSDIR\taskkill.exe" /F /T /IM mautrix-telegram.exe'
  Pop $0
  Pop $1

  ; Windows can keep the final image handle alive briefly after process exit.
  Sleep 1500

  ; Remove only bundled runtime executables from the previous installation.
  ; User data, Matrix data and connector databases live under AppData and are
  ; intentionally untouched.
  ClearErrors
  Delete "$INSTDIR\resources\native\tuwunel.exe"
  ${If} ${Errors}
    Sleep 1500
    ClearErrors
    Delete "$INSTDIR\resources\native\tuwunel.exe"
  ${EndIf}

  ClearErrors
  Delete "$INSTDIR\resources\native\connectors\mautrix-whatsapp.exe"
  ${If} ${Errors}
    Sleep 750
    ClearErrors
    Delete "$INSTDIR\resources\native\connectors\mautrix-whatsapp.exe"
  ${EndIf}

  ClearErrors
  Delete "$INSTDIR\resources\native\connectors\mautrix-telegram.exe"
  ${If} ${Errors}
    Sleep 750
    ClearErrors
    Delete "$INSTDIR\resources\native\connectors\mautrix-telegram.exe"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro UNIBOX_STOP_NATIVE_PROCESSES
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro UNIBOX_STOP_NATIVE_PROCESSES
!macroend
