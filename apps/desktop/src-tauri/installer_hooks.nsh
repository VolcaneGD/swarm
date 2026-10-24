Var SwarmUninstallHelperTemp
Var SwarmUninstallHelperDirectory
!define SWARM_INSTALLER_HELPER "${__FILEDIR__}\..\..\..\scripts\installer_path.ps1"
!macro SWARM_RUN_GUARD helperPath
  ${If} $UpdateMode = 1
    ExecWait '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "${helperPath}" -Action GuardRunning -InstallDirectory "$INSTDIR" -WaitSeconds 30' $0
  ${Else}
    ExecWait '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "${helperPath}" -Action GuardRunning -InstallDirectory "$INSTDIR"' $0
  ${EndIf}
  ${If} $0 != 0
    MessageBox MB_ICONEXCLAMATION|MB_OK "Close Swarm and its background runtime before continuing, then run the installer again. $\r$\n$\r$\nSwarm本体とバックグラウンドRuntimeを終了してから、インストーラーを再実行してください。プロセスは停止していません。"
    Abort
  ${EndIf}
!macroend
!macro NSIS_HOOK_PREINSTALL
  InitPluginsDir
  SetOutPath "$PLUGINSDIR"
  File /oname=swarm-installer.ps1 "${SWARM_INSTALLER_HELPER}"
  !insertmacro SWARM_RUN_GUARD "$PLUGINSDIR\swarm-installer.ps1"
  SetOutPath "$INSTDIR"
!macroend
!macro NSIS_HOOK_POSTINSTALL
  ExecWait '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\swarm-installer.ps1" -Action AddPath -InstallDirectory "$INSTDIR"' $0
  ${If} $0 != 0
    MessageBox MB_ICONEXCLAMATION|MB_OK "Swarm was installed, but its CLI directory could not be registered in your user PATH. Check your Windows user environment and run the installer again. $\r$\n$\r$\nSwarmはインストールされましたが、CLIのフォルダーをユーザーPATHへ登録できませんでした。Windowsユーザー環境を確認し、インストーラーを再実行してください。"
    Abort
  ${EndIf}
!macroend
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro SWARM_RUN_GUARD "$INSTDIR\swarm-installer.ps1"
  GetTempFileName $SwarmUninstallHelperDirectory $TEMP
  Delete "$SwarmUninstallHelperDirectory"
  CreateDirectory "$SwarmUninstallHelperDirectory"
  ClearErrors
  CopyFiles /SILENT "$INSTDIR\swarm-installer.ps1" "$SwarmUninstallHelperDirectory"
  ${If} ${Errors}
    Delete "$SwarmUninstallHelperDirectory\swarm-installer.ps1"
    RMDir "$SwarmUninstallHelperDirectory"
    MessageBox MB_ICONEXCLAMATION|MB_OK "The Swarm installer helper could not be preserved for PATH cleanup. No files were removed; retry after closing Swarm. $\r$\n$\r$\nPATHの後処理用ファイルを準備できませんでした。アンインストールは開始していません。Swarmを終了して再試行してください。"
    Abort
  ${EndIf}
  StrCpy $SwarmUninstallHelperTemp "$SwarmUninstallHelperDirectory\swarm-installer.ps1"
!macroend
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    ExecWait '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$SwarmUninstallHelperTemp" -Action RemovePath -InstallDirectory "$INSTDIR"' $0
    ${If} $0 != 0
      MessageBox MB_ICONEXCLAMATION|MB_OK "Swarm was removed, but its owned user PATH entry could not be removed. Remove only this Swarm install directory from your user PATH: $INSTDIR$\r$\n$\r$\nSwarmは削除されましたが、Swarmが登録したユーザーPATH項目を削除できませんでした。ユーザーPATHからこのSwarmインストール先だけを削除してください: $INSTDIR"
  ${EndIf}
  ${EndIf}
  ; The GUI's current default_data_root() is %LOCALAPPDATA%\Swarm. Tauri's
  ; generated checkbox only removes %APPDATA%\${BUNDLEID} and
  ; %LOCALAPPDATA%\${BUNDLEID}; remove the verified default root only when the
  ; existing explicit checkbox is selected. Custom --data-dir roots and
  ; project-root RULES.md files are outside this path and remain untouched.
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    ClearErrors
    RMDir /r "$LOCALAPPDATA\Swarm"
    ${If} ${Errors}
      MessageBox MB_ICONEXCLAMATION|MB_OK "Swarm was uninstalled, but Windows could not remove all data from $LOCALAPPDATA\Swarm. Check permissions and remove any remaining Swarm data manually. $\r$\n$\r$\nSwarmはアンインストールされましたが、$LOCALAPPDATA\Swarm のデータをすべて削除できませんでした。アクセス許可を確認し、残っているSwarmデータを手動で削除してください。"
    ${EndIf}
  ${EndIf}
  Delete "$SwarmUninstallHelperTemp"
  RMDir "$SwarmUninstallHelperDirectory"
!macroend
