; Crochets de l'installeur Windows (NSIS) de Locaryn.
;
; L'application embarque la commande « locaryn » (locaryn.exe, à côté de
; l'application), mais son dossier n'était sur le PATH de personne : taper
; « locaryn » dans un terminal répondait « n'est pas reconnu ». L'installation
; — et chaque mise à jour, qui repasse par cet installeur — ajoute donc le
; dossier au PATH de l'utilisateur ; la désinstallation l'en retire.
;
; Le travail est fait par locaryn.exe lui-même (apps/cli/src/user_path.rs) :
; les chaînes NSIS s'arrêtent à 1024 caractères et tronqueraient un PATH plus
; long, et PowerShell bloquait l'installation sur une fenêtre figée.

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Ajout de la commande locaryn au PATH de l'utilisateur"
  nsExec::Exec '"$INSTDIR\locaryn.exe" __path-add'
  Pop $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Retrait de la commande locaryn du PATH de l'utilisateur"
  nsExec::Exec '"$INSTDIR\locaryn.exe" __path-remove'
  Pop $0
!macroend
