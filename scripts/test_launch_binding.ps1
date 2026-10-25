$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'launch_binding.ps1')
function Assert-True($Condition, $Message) { if (-not $Condition) { throw $Message } }
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('swarm-launch-test-' + [guid]::NewGuid().ToString('N'))
$oldInstall = Join-Path $fixture 'Old Install'
$newInstall = Join-Path $fixture 'New Install'
$data = Join-Path $fixture 'Original Data'
$otherData = Join-Path $fixture 'Other Data'
foreach ($path in @($oldInstall, $newInstall, $data, $otherData)) { [IO.Directory]::CreateDirectory($path) | Out-Null }
$sentinel = Join-Path $data 'preserve.txt'
[IO.File]::WriteAllText($sentinel, 'original-data')
$desktop = Join-Path $fixture 'Desktop.lnk'
$menu = Join-Path $fixture 'Menu.lnk'
$state = Join-Path $fixture 'capture.json'
$shell = New-Object -ComObject WScript.Shell
function Write-TestLink($Path, $Target, $Arguments) {
    $link = $shell.CreateShortcut($Path)
    $link.TargetPath = $Target
    $link.Arguments = $Arguments
    $link.Save()
}
# Reproduce relocation: the old Desktop has Data; NSIS replaces Menu with no args.
Write-TestLink $desktop (Join-Path $oldInstall 'swarm-desktop.exe') ('--data-dir "' + $data + '"')
Write-TestLink $menu (Join-Path $oldInstall 'swarm-desktop.exe') ''
Capture-SwarmLaunchBinding $newInstall $state @($desktop, $menu)
# The old uninstaller can remove both links between GUIINIT and PREINSTALL.
[IO.File]::Delete($desktop)
[IO.File]::Delete($menu)
Capture-SwarmLaunchBinding $newInstall $state @($desktop, $menu)
Write-TestLink $desktop (Join-Path $oldInstall 'swarm-desktop.exe') ''
Write-TestLink $menu (Join-Path $newInstall 'swarm-desktop.exe') ''
Apply-SwarmLaunchBinding $newInstall $state @($desktop, $menu)
foreach ($path in @($desktop, $menu)) {
    $link = $shell.CreateShortcut($path)
    Assert-True ($link.TargetPath -eq (Join-Path $newInstall 'swarm-desktop.exe')) 'Shortcut did not target relocated GUI'
    Assert-True ((Get-SwarmShortcutRoot $link) -eq $data) 'Shortcut did not preserve Data'
}
Assert-True ((Read-SwarmLaunchBinding (Join-Path $newInstall 'swarm-launch.json')) -eq $data) 'No-argument GUI launch lost Data'
Assert-True ([IO.File]::ReadAllText($sentinel) -ceq 'original-data') 'Existing Data changed'
# Conflicting roots must fail before writing capture/applying any Data selection.
Write-TestLink $menu (Join-Path $newInstall 'swarm-desktop.exe') ('--data-dir "' + $otherData + '"')
$rejected = $false
try { Capture-SwarmLaunchBinding $newInstall (Join-Path $fixture 'conflict.json') @($desktop, $menu) } catch { $rejected = $true }
Assert-True $rejected 'Conflicting Data selections were silently chosen'
Assert-True (-not [IO.File]::Exists((Join-Path $fixture 'conflict.json'))) 'Conflict wrote capture state'
Write-TestLink $menu (Join-Path $newInstall 'swarm-desktop.exe') '--data-dir "C:\missing-swarmed-data" --unknown'
$rejected = $false
try { Capture-SwarmLaunchBinding $newInstall (Join-Path $fixture 'invalid.json') @($menu) } catch { $rejected = $true }
Assert-True $rejected 'Arbitrary arguments accepted'
# Fresh installation does not invent a Data root or create a Data directory.
$fresh = Join-Path $fixture 'Fresh Install'
[IO.Directory]::CreateDirectory($fresh) | Out-Null
Capture-SwarmLaunchBinding $fresh (Join-Path $fixture 'fresh.json') @()
Apply-SwarmLaunchBinding $fresh (Join-Path $fixture 'fresh.json') @()
Assert-True (-not [IO.File]::Exists((Join-Path $fresh 'swarm-launch.json'))) 'Fresh installation invented a selection'
# Native Windows argv round-trip, including a drive root and trailing separators.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class SwarmLaunchArgvTest {
    [DllImport("shell32.dll", CharSet=CharSet.Unicode)] static extern IntPtr CommandLineToArgvW(string command, out int count);
    [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr memory);
    public static string[] Parse(string command) {
        int count; var memory=CommandLineToArgvW(command,out count);
        if(memory==IntPtr.Zero) throw new Exception("Native argv parsing failed");
        try { var values=new string[count]; for(int i=0;i<count;i++) values[i]=Marshal.PtrToStringUni(Marshal.ReadIntPtr(memory,i*IntPtr.Size)); return values; }
        finally { LocalFree(memory); }
    }
}
'@
foreach ($root in @(($data + '\'), ([IO.Path]::GetPathRoot($fixture)))) {
    $arguments = ConvertTo-SwarmDataArguments $root
    $parsed = [SwarmLaunchArgvTest]::Parse('swarm-desktop.exe ' + $arguments)
    Assert-True ($parsed.Length -eq 3 -and $parsed[2] -ceq $root) 'Native trailing separator argument corrupted'
    Write-TestLink $menu (Join-Path $newInstall 'swarm-desktop.exe') $arguments
    Assert-True ((Get-SwarmShortcutRoot ($shell.CreateShortcut($menu))) -eq ([IO.Path]::GetFullPath($root))) 'Captured native argument corrupted'
}
Write-Output ('PASS relocation/shortcut preservation/conflict/invalid/fresh/Data sentinel; fixture retained: ' + $fixture)
