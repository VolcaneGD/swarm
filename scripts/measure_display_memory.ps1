# Read-only process observations. No UI automation or working-set trimming.
param([Parameter(Mandatory)][int]$GuiProcessId,[Parameter(Mandatory)][int]$RuntimeProcessId,[Parameter(Mandatory)][string]$OutputPath,[ValidateRange(1,120)][int]$Samples=6)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputPath){throw 'Preserve prior observation'}
$displayRoot=Get-CimInstance Win32_Process -Filter "ProcessId=$GuiProcessId"
if(!$displayRoot -or $displayRoot.Name -ne 'swarm-desktop.exe'){throw 'Exact live GUI required'}
$displayCreated=$displayRoot.CreationDate
$displayReadings=@()
for($displaySample=0;$displaySample -lt $Samples;$displaySample++){
 $displayNodes=@(Get-CimInstance Win32_Process)
 $displayIds=[Collections.Generic.HashSet[int]]::new();[void]$displayIds.Add($GuiProcessId)
 do{$displayAdded=$false;foreach($displayNode in $displayNodes){if($displayNode.CreationDate -ge $displayCreated -and $displayIds.Contains([int]$displayNode.ParentProcessId) -and $displayIds.Add([int]$displayNode.ProcessId)){$displayAdded=$true}}}while($displayAdded)
 $displayProcesses=@(foreach($displayNode in $displayNodes){if($displayIds.Contains([int]$displayNode.ProcessId)){
   $displayProcess=Get-Process -Id $displayNode.ProcessId -ErrorAction SilentlyContinue
   if($displayProcess){$displayRole='other';if($displayNode.Name -eq 'swarm-desktop.exe'){$displayRole='gui'}elseif($displayNode.Name -eq 'swarm-runtime.exe'){$displayRole='runtime'}elseif($displayNode.Name -eq 'msedgewebview2.exe'){$displayRole='browser';if($displayNode.CommandLine -match '--type=(renderer|gpu-process|utility|crashpad-handler)\b'){$displayRole=$Matches[1]}}
    [pscustomobject][ordered]@{pid=[int]$displayNode.ProcessId;name=$displayNode.Name;role=$displayRole;working_set_bytes=$displayProcess.WorkingSet64;private_bytes=$displayProcess.PrivateMemorySize64}
   }
 }})
 $displayRuntime=Get-Process -Id $RuntimeProcessId -ErrorAction Stop
 if($displayProcesses.Count -eq 0){throw 'No processes measured; not a memory PASS'}
 $displayReadings+=,[pscustomobject][ordered]@{sample=$displaySample;observed_utc=(Get-Date).ToUniversalTime().ToString('o');gui_and_descendant_working_set_bytes=($displayProcesses|Measure-Object working_set_bytes -Sum).Sum;gui_and_descendant_private_bytes=($displayProcesses|Measure-Object private_bytes -Sum).Sum;runtime_working_set_bytes=$displayRuntime.WorkingSet64;processes=$displayProcesses}
 Start-Sleep -Milliseconds 500
}
$displayPeak=($displayReadings|Measure-Object gui_and_descendant_working_set_bytes -Maximum).Maximum
if($null -eq $displayPeak -or $displayPeak -le 0){throw 'Missing memory measurement; not a PASS'}
$displayResult=[ordered]@{gui_pid=$GuiProcessId;runtime_pid=$RuntimeProcessId;gui_creation=$displayCreated.ToUniversalTime().ToString('o');metric='sum of GUI and descendant WorkingSet64; same target as earlier observations';gui_target_bytes=536870912;runtime_target_bytes=268435456;peak_gui_bytes=$displayPeak;gui_target_met=($displayPeak -le 536870912);samples=$displayReadings}
[IO.File]::WriteAllText($OutputPath,($displayResult|ConvertTo-Json -Depth 6),[Text.UTF8Encoding]::new($false))
[ordered]@{peak_gui_bytes=$displayPeak;gui_target_met=($displayPeak -le 536870912);samples=$Samples}|ConvertTo-Json -Compress
