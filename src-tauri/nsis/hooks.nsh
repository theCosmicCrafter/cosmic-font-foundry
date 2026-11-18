; NSIS installer hooks for Cosmic Font Foundry (bundle.windows.nsis.installerHooks).

!define CFF_NAME "Cosmic Font Foundry"

!macro NSIS_HOOK_PREINSTALL
  SetOutPath $INSTDIR
!macroend

!macro NSIS_HOOK_POSTINSTALL
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Clean up Task Manager startup approval flag unless running an in-place update.
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${CFF_NAME}"
  ${EndIf}
!macroend
