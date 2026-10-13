; Windows-Firewall: CDJ-Netze ohne Router gelten als öffentlich. Ohne Freigabe
; für alle Profile verstummen die Player nach 20–30 s. Alte Regeln für das
; Programm (auch eine Sperre nach „Abbrechen“ im Firewall-Dialog) vorher löschen.
!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name=all program="$INSTDIR\${MAINBINARYNAME}.exe"'
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name=DeckLight dir=in action=allow program="$INSTDIR\${MAINBINARYNAME}.exe" enable=yes profile=any'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name=DeckLight'
!macroend
