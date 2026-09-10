; Unibox NSIS lifecycle hooks.
; Every release ships an immutable native resource slot (native-v050, ...).
; Upgrades therefore stop running images but never delete or migrate user data.

!macro UNIBOX_KILL_IMAGE IMAGE
  nsExec::ExecToStack '"$SYSDIR\taskkill.exe" /F /T /IM ${IMAGE}'
  Pop $0
  Pop $1
!macroend

!macro UNIBOX_STOP_NATIVE_PROCESSES
  DetailPrint "Stopping Unibox native runtime processes before update..."

  !insertmacro UNIBOX_KILL_IMAGE "unibox-desktop.exe"
  !insertmacro UNIBOX_KILL_IMAGE "tuwunel.exe"
  !insertmacro UNIBOX_KILL_IMAGE "mautrix-whatsapp.exe"
  !insertmacro UNIBOX_KILL_IMAGE "mautrix-telegram.exe"
  !insertmacro UNIBOX_KILL_IMAGE "mautrix-signal.exe"
  !insertmacro UNIBOX_KILL_IMAGE "mautrix-discord.exe"
  !insertmacro UNIBOX_KILL_IMAGE "mautrix-instagram.exe"
  !insertmacro UNIBOX_KILL_IMAGE "mautrix-meta.exe"
  !insertmacro UNIBOX_KILL_IMAGE "mautrix-gmessages.exe"
  !insertmacro UNIBOX_KILL_IMAGE "mautrix-googlechat.exe"
  !insertmacro UNIBOX_KILL_IMAGE "mautrix-slack.exe"
  !insertmacro UNIBOX_KILL_IMAGE "mautrix-twitter.exe"
  !insertmacro UNIBOX_KILL_IMAGE "mautrix-bluesky.exe"
  !insertmacro UNIBOX_KILL_IMAGE "mautrix-linkedin.exe"
  !insertmacro UNIBOX_KILL_IMAGE "mautrix-gvoice.exe"
  !insertmacro UNIBOX_KILL_IMAGE "mautrix-zulip.exe"
  !insertmacro UNIBOX_KILL_IMAGE "mautrix-irc.exe"

  ; Let Windows release final executable handles before Tauri replaces the app.
  Sleep 1500
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro UNIBOX_STOP_NATIVE_PROCESSES
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro UNIBOX_STOP_NATIVE_PROCESSES
!macroend
