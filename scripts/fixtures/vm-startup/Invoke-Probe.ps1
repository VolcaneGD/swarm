# One hash-pinned observer, not a VM manager. Passed as a scriptblock to Direct.
function Invoke-StartupProbe {
 param([string]$Program,[string[]]$Arguments,[string]$ExpectedHash,[string]$TraceDirectory,[string]$TraceName,[int]$DeadlineMilliseconds=170000)
 $ErrorActionPreference='Stop'
 # Do not inherit a PowerShell7-only module search path in a PS5.1 child.
 Import-Module "$PSHOME\Modules\Microsoft.PowerShell.Utility\Microsoft.PowerShell.Utility.psd1" -ErrorAction Stop
 Import-Module "$PSHOME\Modules\CimCmdlets\CimCmdlets.psd1" -ErrorAction Stop
 if($DeadlineMilliseconds -le 0 -or $TraceName -notmatch '^[a-z0-9-]+$'){throw 'Invalid bounded probe invocation'}
 if((Get-FileHash -LiteralPath $Program).Hash -cne $ExpectedHash){throw 'Probe hash changed'}
 foreach($argument in $Arguments){if($argument -match '[\s"\x00]'){throw 'Probe arguments must be reviewed single tokens'}}
 function Write-ProbeTrace($suffix,$value){
  $bytes=(New-Object Text.UTF8Encoding($false)).GetBytes(($value|ConvertTo-Json -Depth 5))
  $file=[IO.File]::Open((Join-Path $TraceDirectory "$TraceName-$suffix.json"),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
  try{$file.Write($bytes,0,$bytes.Length)}finally{$file.Dispose()}
 }
 # Exclusive marker precedes starting the parent, including on timeout.
 Write-ProbeTrace 'attempt' @{automatic_retries=0;at=[DateTimeOffset]::Now.ToString('o')}
 $info=New-Object Diagnostics.ProcessStartInfo
 $info.FileName=$Program;$info.Arguments=$Arguments -join ' '
 $info.UseShellExecute=$false;$info.CreateNoWindow=$true
 $info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
 $process=New-Object Diagnostics.Process;$process.StartInfo=$info
 $clock=[Diagnostics.Stopwatch]::StartNew();$phase='start';$pidValue=$null
 try{
  if(!$process.Start()){throw 'Probe did not start'}
  $null=$process.Handle;$pidValue=$process.Id
  Write-ProbeTrace 'started' @{pid=$pidValue}
  $stdout=$process.StandardOutput.ReadToEndAsync();$stderr=$process.StandardError.ReadToEndAsync()
  $phase='parent-exit'
  if(!$process.WaitForExit($DeadlineMilliseconds)){throw 'Probe parent deadline; retain process, no retry'}
  $code=$process.ExitCode
  Write-ProbeTrace 'parent-exit' @{pid=$pidValue;exit_code=$code;elapsed_ms=$clock.ElapsedMilliseconds}
  foreach($pair in @(@('stdout-eof',$stdout),@('stderr-eof',$stderr))){
   $phase=$pair[0];$remaining=[Math]::Max(0,$DeadlineMilliseconds-[int]$clock.ElapsedMilliseconds)
   if(!$pair[1].Wait($remaining)){throw 'Probe stream deadline after parent exit; preserve state'}
  }
  $outText=$stdout.GetAwaiter().GetResult();$errText=$stderr.GetAwaiter().GetResult()
  if($outText.Length -gt 65536 -or $errText.Length -gt 65536){throw 'Probe output bound exceeded'}
  Write-ProbeTrace 'streams-closed' @{pid=$pidValue;exit_code=$code;stdout_bytes=[Text.Encoding]::UTF8.GetByteCount($outText);stderr_bytes=[Text.Encoding]::UTF8.GetByteCount($errText);elapsed_ms=$clock.ElapsedMilliseconds}
  $phase='exit-code'
  if($code -ne 0){throw 'Probe returned failure; no retry'}
  $phase='json-decode'
  $value=$outText|ConvertFrom-Json
  $directory=Split-Path -Parent $Program
  $phase='process-inventory'
  Write-ProbeTrace 'inventory-start' @{pid=$pidValue;elapsed_ms=$clock.ElapsedMilliseconds}
  $ownedProcesses=@(Get-CimInstance Win32_Process|Where-Object{$_.ExecutablePath -in @("$directory\swarm-runtime.exe","$directory\swarm-desktop.exe")}|Select-Object ProcessId,Name,SessionId)
  if($clock.ElapsedMilliseconds -ge $DeadlineMilliseconds){throw 'Process inventory exceeded probe deadline'}
  Write-ProbeTrace 'complete' @{pid=$pidValue;owned_process_count=$ownedProcesses.Count;elapsed_ms=$clock.ElapsedMilliseconds}
  @{product=$value;processes=$ownedProcesses;at=[DateTimeOffset]::Now.ToString('o')}
 }catch{
  Write-ProbeTrace 'failure' @{phase=$phase;pid=$pidValue;elapsed_ms=$clock.ElapsedMilliseconds;automatic_retries=0;process_exit_not_assumed=$true}
  throw
 }finally{$process.Dispose()}
}
