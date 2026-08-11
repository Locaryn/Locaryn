; Locaryn Server — installeur Windows (NSIS).
;
; Installe le démon et la CLI sans interface dans Program Files, ajoute la
; CLI au PATH, et enregistre une entrée de désinstallation propre.
;
; Compilation par la CI (workflow release) :
;   makensis /DVERSION=<version> services/daemon/windows/locaryn-server.nsi
; Les binaires doivent être copiés à côté de ce script AVANT compilation :
; le script les référence par chemins absolus (${__FILEDIR__}), donc le
; répertoire courant de makensis n'a pas d'importance.

Unicode true
!include "LogicLib.nsi"

!ifndef VERSION
  !define VERSION "0.1.0"
!endif
!define APP_VERSION "${VERSION}"
!define APP_NAME "Locaryn Server"
!define APP_PUBLISHER "Locaryn Contributors"
!define APP_UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\LocarynServer"
!define APP_INSTDIR "$PROGRAMFILES64\Locaryn Server"
!define APP_SRCDIR "${__FILEDIR__}"

Name "${APP_NAME}"
OutFile "${APP_SRCDIR}\locaryn-${APP_VERSION}-server-windows-x64-setup.exe"
InstallDir "${APP_INSTDIR}"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

; Ajoute $INSTDIR au PATH système s'il n'y est pas déjà.
; Attend $0 = valeur actuelle du PATH.
!macro AddToPath
  Push "$0"
  Push "$INSTDIR"
  Call StrStr
  Pop $1
  ${If} $1 == ""
    ${If} $0 == ""
      StrCpy $0 "$INSTDIR"
    ${Else}
      StrCpy $0 "$0;$INSTDIR"
    ${EndIf}
    WriteRegExpandStr HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path" "$0"
  ${EndIf}
!macroend

; Retire $INSTDIR du PATH système.
!macro RemoveFromPath
  ReadRegStr $0 HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path"
  ${If} $0 != ""
    Push "$0"
    Push ";$INSTDIR"
    Call StrStr
    Pop $1
    ${If} $1 != ""
      StrLen $2 ";$INSTDIR"
      StrLen $3 "$1"
      IntOp $4 $3 - $2
      StrCpy $5 "$0" $4
      StrCpy $6 "$0" "" $3
      StrCpy $0 "$5$6"
      WriteRegExpandStr HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path" "$0"
    ${EndIf}
  ${EndIf}
!macroend

Section "Install"
  SetOutPath "$INSTDIR"
  File "${APP_SRCDIR}\locaryn-daemon.exe"
  File "${APP_SRCDIR}\locaryn.exe"
  File /nonfatal "${APP_SRCDIR}\README.md"
  File /nonfatal "${APP_SRCDIR}\LICENSE"
  File /nonfatal "${APP_SRCDIR}\LICENSES.md"

  WriteUninstaller "$INSTDIR\Uninstall.exe"

  WriteRegStr HKLM "${APP_UNINST_KEY}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKLM "${APP_UNINST_KEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKLM "${APP_UNINST_KEY}" "Publisher" "${APP_PUBLISHER}"
  WriteRegStr HKLM "${APP_UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${APP_UNINST_KEY}" "DisplayIcon" "$INSTDIR\locaryn-daemon.exe"
  WriteRegStr HKLM "${APP_UNINST_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKLM "${APP_UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${APP_UNINST_KEY}" "NoRepair" 1

  ReadRegStr $0 HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path"
  !insertmacro AddToPath
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\locaryn-daemon.exe"
  Delete "$INSTDIR\locaryn.exe"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\LICENSES.md"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"

  !insertmacro RemoveFromPath

  DeleteRegKey HKLM "${APP_UNINST_KEY}"
SectionEnd

; ── StrStr : renvoie la sous-chaîne de $0 à partir de la 1re occurrence de $1 ──
Function StrStr
  Exch $1
  Exch
  Exch $0
  Exch
  Push $2
  Push $3
  Push $4
  StrCpy $2 -1
Loop:
  IntOp $2 $2 + 1
  StrCpy $3 $0 $2
  StrCmp $3 "" EndLoop
  StrCmp $3 $1 Found
  Goto Loop
Found:
  StrCpy $4 $0 "" $2
  Exch
  Pop $3
  Pop $2
  Pop $1
  Pop $0
  Exch $4
  Goto Done
EndLoop:
  StrCpy $4 ""
  Exch
  Pop $3
  Pop $2
  Pop $1
  Pop $0
  Exch $4
Done:
FunctionEnd
