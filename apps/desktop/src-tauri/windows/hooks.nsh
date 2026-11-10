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

; Le service (mode serveur), la CLI et locaryn-server tournent à côté de
; l'application, depuis ce même dossier : tant qu'ils vivent, Windows refuse
; de remplacer leur fichier, et la mise à jour s'arrêtait sur « Erreur lors de
; l'ouverture du fichier en écriture : locaryn-daemon.exe ». On les arrête
; avant de copier ; l'application relance le service au démarrage si le mode
; serveur était allumé.
; Attendre qu'un fichier du dossier d'installation soit libre (20 s au plus).
; Un processus tué met un instant à rendre son fichier, un antivirus qui
; l'analyse aussi : copier tout de suite échouait sur « ouverture du fichier
; en écriture ».
!macro LOCARYN_ATTENDRE_LIBRE FICHIER
  Push $1
  Push $2
  StrCpy $1 0
  ${Do}
    ${IfNot} ${FileExists} "${FICHIER}"
      ${Break}
    ${EndIf}
    ClearErrors
    FileOpen $2 "${FICHIER}" a
    ${IfNot} ${Errors}
      FileClose $2
      ${Break}
    ${EndIf}
    IntOp $1 $1 + 1
    ${If} $1 > 40
      ${Break}
    ${EndIf}
    Sleep 500
  ${Loop}
  Pop $2
  Pop $1
!macroend

; L'application elle-même aussi : une mise à jour lancée pendant qu'elle se
; ferme (elle arrête ses moteurs en sortant), une seconde instance ou un
; processus resté en arrière-plan gardaient locaryn-desktop.exe verrouillé.
!macro LOCARYN_ARRETER_COMPAGNONS
  DetailPrint "Arret de Locaryn, de son service et de ses commandes"
  nsExec::Exec 'taskkill /F /T /IM locaryn-daemon.exe'
  Pop $0
  nsExec::Exec 'taskkill /F /T /IM locaryn-server.exe'
  Pop $0
  nsExec::Exec 'taskkill /F /T /IM locaryn.exe'
  Pop $0
  ; Sans /T : lancé par la mise à jour automatique, l'installeur est un enfant
  ; de l'application et serait tué avec elle.
  nsExec::Exec 'taskkill /F /IM locaryn-desktop.exe'
  Pop $0
  !insertmacro LOCARYN_ATTENDRE_LIBRE "$INSTDIR\locaryn-desktop.exe"
  !insertmacro LOCARYN_ATTENDRE_LIBRE "$INSTDIR\locaryn-daemon.exe"
  !insertmacro LOCARYN_ATTENDRE_LIBRE "$INSTDIR\locaryn-server.exe"
  !insertmacro LOCARYN_ATTENDRE_LIBRE "$INSTDIR\locaryn.exe"
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro LOCARYN_ARRETER_COMPAGNONS
!macroend

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Ajout de la commande locaryn au PATH de l'utilisateur"
  nsExec::Exec '"$INSTDIR\locaryn.exe" __path-add'
  Pop $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro LOCARYN_ARRETER_COMPAGNONS
  DetailPrint "Retrait de la commande locaryn du PATH de l'utilisateur"
  nsExec::Exec '"$INSTDIR\locaryn.exe" __path-remove'
  Pop $0
!macroend
