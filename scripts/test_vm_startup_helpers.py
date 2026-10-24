"""New synthetic process/guard regression only. No Hyper-V, product or fixture run."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
PS = Path(os.environ.get('WINDIR', 'C:/Windows')) / 'System32/WindowsPowerShell/v1.0/powershell.exe'
RUNNER = ROOT / 'scripts/fixtures/vm-startup/Invoke-Probe.ps1'

def quote(value):
    return "'" + str(value).replace("'", "''") + "'"

class StartupHelpers(unittest.TestCase):
    def powershell(self, command):
        command = 'Import-Module "$PSHOME/Modules/Microsoft.PowerShell.Utility/Microsoft.PowerShell.Utility.psd1"; ' + command
        p = subprocess.run([str(PS), '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-Command', command], capture_output=True, text=True, timeout=20)
        self.assertEqual(p.returncode, 0, p.stderr)
        return p.stdout.strip()

    def child_fixture(self, directory, inherit=False):
        script = directory / 'synthetic.py'
        script.write_text(
            'import json,subprocess,sys\n'
            "c=subprocess.Popen([sys.executable,'-c','import time; time.sleep(3)'],stdin=subprocess.DEVNULL,"
            + ('stdout=None,stderr=None,' if inherit else 'stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,')
            + 'creationflags=0x08000000)\n'
            + f"open({str(directory / 'parent-complete.json')!r},'w').write(json.dumps({{'child_pid':c.pid}}))\n"
            + "print(json.dumps({'synthetic':True,'child_pid':c.pid}))\n", encoding='utf-8')
        return script

    def test_parent_completion_does_not_wait_for_background_child(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            fixture = self.child_fixture(directory)
            command = f". {quote(RUNNER)}; $h=(Get-FileHash -LiteralPath {quote(sys.executable)}).Hash; $r=Invoke-StartupProbe -Program {quote(sys.executable)} -Arguments @({quote(fixture)}) -ExpectedHash $h -TraceDirectory {quote(directory)} -TraceName detached -DeadlineMilliseconds 2000; $r.product | ConvertTo-Json -Compress"
            try:
                result = json.loads(self.powershell(command))
                self.assertTrue(result['synthetic'])
                trace = json.loads((directory / 'detached-parent-exit.json').read_text())
                self.assertEqual(trace['exit_code'], 0)
                self.assertLess(trace['elapsed_ms'], 2000)
                self.assertTrue((directory / 'detached-complete.json').is_file())
            finally:
                time.sleep(3.2)  # Only this synthetic child; natural exit, no kill.

    def test_inherited_stream_timeout_is_distinguished_from_parent_timeout(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            fixture = self.child_fixture(directory, inherit=True)
            command = f". {quote(RUNNER)}; $h=(Get-FileHash -LiteralPath {quote(sys.executable)}).Hash; try {{Invoke-StartupProbe -Program {quote(sys.executable)} -Arguments @({quote(fixture)}) -ExpectedHash $h -TraceDirectory {quote(directory)} -TraceName inherited -DeadlineMilliseconds 1000 | Out-Null; throw 'unexpected success'}} catch {{if(!(Test-Path -LiteralPath {quote(directory / 'inherited-failure.json')})){{throw}}; Get-Content -LiteralPath {quote(directory / 'inherited-failure.json')} -Raw}}"
            try:
                failure = json.loads(self.powershell(command))
                self.assertEqual(failure['phase'], 'stdout-eof')
                self.assertEqual(failure['automatic_retries'], 0)
                self.assertTrue((directory / 'inherited-parent-exit.json').is_file())
                self.assertFalse((directory / 'inherited-complete.json').exists())
            finally:
                time.sleep(3.2)

    def test_consumed_scope_refuses_before_pin_repair_or_vm_access(self):
        for mode in ('-Prepare', '-ValidateOnly', '-ExecuteApproved'):
            with self.subTest(mode=mode):
                p = subprocess.run([str(PS), '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', str(ROOT / 'scripts/vm_startup_sequence.ps1'), mode], capture_output=True, text=True, timeout=10)
                self.assertNotEqual(p.returncode, 0)
                self.assertIn('Added sequence consumed', p.stderr)

    def test_record_legacy_native_job_behavior_without_product_or_vm(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            fixture = self.child_fixture(directory)
            marker = directory / 'parent-complete.json'
            command = f"$j=Start-Job -ScriptBlock {{param($p,$f) & $p $f}} -ArgumentList {quote(sys.executable)},{quote(fixture)}; $until=[DateTime]::UtcNow.AddSeconds(8); while(!(Test-Path -LiteralPath {quote(marker)})){{if([DateTime]::UtcNow -gt $until){{throw 'synthetic parent not ready'}}; Start-Sleep -Milliseconds 20}}; $done=[bool](Wait-Job -Job $j -Timeout 1); $state=$j.State.ToString(); if(!(Wait-Job -Job $j -Timeout 5)){{throw 'synthetic descendant did not exit naturally'}}; $null=Receive-Job -Job $j; Remove-Job -Job $j; @{{parent_receipt_exists=$true;job_state_after_parent_receipt=$state;completed_within_one_second=$done}}|ConvertTo-Json -Compress"
            observation = json.loads(self.powershell(command))
            self.assertTrue(observation['parent_receipt_exists'])
            print('Legacy native job observation: ' + json.dumps(observation))
            time.sleep(3.2)  # Synthetic descendant exits naturally before return.

if __name__ == '__main__':
    unittest.main()
