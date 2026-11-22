; PC Optimizer installer hooks (Stage 3: 3.15).
;
; Before the files are removed, the uninstaller undoes every change the app made, with
; the bundled `pco.exe undo --all` (the same engine and journal as the app). If something
; cannot be undone, the user decides whether to uninstall anyway; a silent uninstall stops
; instead, so nothing is left changed without a journal to undo it.
;
; Not on an update: Tauri's installer runs the previous uninstaller with /UPDATE, and the
; user's tweaks must survive an update.

!include "FileFunc.nsh"

!macro NSIS_HOOK_PREUNINSTALL
  ${GetOptions} $CMDLINE "/UPDATE" $R9
  IfErrors 0 pco_skip_undo
  DetailPrint "Undoing every change made by PC Optimizer..."
  nsExec::ExecToLog '"$INSTDIR\pco.exe" --lang en undo --all'
  Pop $R8
  StrCmp $R8 "0" pco_skip_undo
  MessageBox MB_YESNO|MB_ICONEXCLAMATION "Some changes made by PC Optimizer could not be undone (code $R8). Open the app and use Settings > Undo all to see why.$\r$\n$\r$\nUninstall anyway?" /SD IDNO IDYES pco_skip_undo
  Abort
  pco_skip_undo:
!macroend
