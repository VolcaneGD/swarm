"""Local safety checks only. No VM access, credential prompt, or guest operation."""
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "vm_retained_continuation.ps1"
OUT = ROOT / "target" / "vm-retained-20261020"


def call(*args):
    return subprocess.run(
        ["powershell.exe", "-NoProfile", "-NonInteractive", "-File", str(SCRIPT), *args],
        cwd=ROOT, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=15,
    )


class RetainedContinuationTests(unittest.TestCase):
    def test_restricted_parent_loads_helper_only_inside_process_scoped_child(self):
        with tempfile.TemporaryDirectory(prefix='swarm-policy-boundary-') as temp:
            helper = Path(temp) / 'helper.ps1'
            helper.write_text("function Get-OwnedProbe { 'bounded-helper-loaded' }", encoding='ascii')
            helper_literal = str(helper).replace("'", "''")
            child = f"$ErrorActionPreference='Stop'; . '{helper_literal}'; Get-OwnedProbe"
            import base64
            encoded = base64.b64encode(child.encode('utf-16-le')).decode('ascii')
            command = (
                "$ErrorActionPreference='Stop'; "
                "Import-Module \"$PSHOME\\Modules\\Microsoft.PowerShell.Security\\Microsoft.PowerShell.Security.psd1\"; "
                "Import-Module \"$PSHOME\\Modules\\Microsoft.PowerShell.Utility\\Microsoft.PowerShell.Utility.psd1\"; "
                "Set-ExecutionPolicy -Scope Process -ExecutionPolicy Restricted -Force; "
                "if((Get-ExecutionPolicy) -ne 'Restricted'){throw 'Restricted parent required'}; "
                "$before=Get-ExecutionPolicy -List|ConvertTo-Json -Compress; "
                f"$value=& powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand '{encoded}'; "
                "if($LASTEXITCODE -ne 0){throw 'Child failed'}; "
                "if((Get-ExecutionPolicy -List|ConvertTo-Json -Compress) -cne $before){throw 'Parent policy changed'}; "
                "$value"
            )
            result = subprocess.run(
                ['powershell.exe', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-Command', command],
                capture_output=True, text=True, timeout=15,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), 'bounded-helper-loaded')

    def test_fourth_continuation_binds_recovered_identity_and_original_inputs(self):
        fourth = ROOT / 'target/vm-retained-20261020-extension-4'
        if not (fourth / 'approval.json').exists():
            self.skipTest('Requires actual fourth continuation approval')
        before = sorted(p.name for p in fourth.iterdir())
        result = call('-Continuation4', '-ValidateApprovalOnly')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), {
            'approval_matches': True, 'vm_access': False,
            'attempt_consumed': (fourth / 'attempt.json').exists(),
        })
        plan = json.loads(call('-Continuation4', '-ShowPlan').stdout)
        self.assertIn('identity-connection-result', plan['input_hashes'])
        ambiguous = call('-Continuation3', '-Continuation4', '-ShowPlan')
        self.assertNotEqual(ambiguous.returncode, 0)
        self.assertIn('Choose one explicit continuation', ambiguous.stderr)
        if (fourth / 'attempt.json').exists():
            consumed = call('-Continuation4', '-ExecuteApproved')
            self.assertNotEqual(consumed.returncode, 0)
            self.assertIn('VM extension consumed', consumed.stderr)
        self.assertEqual(sorted(p.name for p in fourth.iterdir()), before)

    def test_explicit_second_continuation_preserves_prior_consumed_evidence(self):
        second = ROOT / 'target/vm-retained-20261020-extension-2'
        if not (second / 'approval.json').exists():
            self.skipTest('Requires actual renewed human instruction')
        before = sorted(p.name for p in second.iterdir())
        approval = json.loads((second / 'approval.json').read_text(encoding='utf-8'))
        result = call('-Continuation2', '-ValidateApprovalOnly')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), {
            'approval_matches': True, 'vm_access': False,
            'attempt_consumed': (second / 'attempt.json').exists(),
        })
        plan = json.loads(call('-Continuation2', '-ShowPlan').stdout)
        for name in ('attempt.json', 'result.json'):
            self.assertEqual(plan['input_hashes']['prior-' + name], approval['prior_receipt_hashes'][name])
        if (second / 'attempt.json').exists():
            consumed = call('-Continuation2', '-ExecuteApproved')
            self.assertNotEqual(consumed.returncode, 0)
            self.assertIn('VM extension consumed', consumed.stderr)
        self.assertEqual(sorted(p.name for p in second.iterdir()), before)

    @unittest.skipUnless((OUT / "approval.json").exists(), "Requires actual recorded human approval")
    def test_exact_unicode_approval_without_vm_access(self):
        self.assertTrue(SCRIPT.read_bytes().isascii(), "PS5.1 source must preserve the Japanese comparison")
        self.assertTrue((OUT / "approval.json").exists(), "Run this case after real approval only")
        before = sorted(p.name for p in OUT.iterdir())
        result = call("-ValidateApprovalOnly")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), {
            "approval_matches": True, "vm_access": False,
            "attempt_consumed": (OUT / "attempt.json").exists(),
        })
        self.assertEqual(sorted(p.name for p in OUT.iterdir()), before)

    def test_resource_preflight_does_not_open_vm_or_mutate(self):
        before = sorted(p.name for p in OUT.iterdir())
        result = call("-InspectResources")
        self.assertEqual(result.returncode, 0, result.stderr)
        value = json.loads(result.stdout)
        self.assertEqual(value["startup_bytes"], 6 * 1024**3)
        self.assertEqual(value["host_margin_bytes"], 1024**3)
        self.assertIs(value["vm_access"], False)
        self.assertIs(value["vm_mutation"], False)
        required = 7 * 1024**3
        self.assertEqual(value["required_available_bytes"], required)
        self.assertEqual(value["can_start"], value["available_ram_bytes"] >= required and value["free_commit_bytes"] >= required)
        self.assertEqual(sorted(p.name for p in OUT.iterdir()), before)

    def test_reviewable_plan_pins_actual_retained_inputs(self):
        result = call("-ShowPlan")
        self.assertEqual(result.returncode, 0, result.stderr)
        plan = json.loads(result.stdout)
        proof = json.loads((ROOT / "target/batched-roadmap-20261015-os/vm-ready.json").read_text(encoding="utf-8-sig"))
        self.assertEqual(plan["vm_id"], proof["vm_id"])
        self.assertEqual(plan["baseline_id"], proof["baseline_id"])
        self.assertEqual(plan["input_hashes"]["package-report.json"], proof["input_sha256"][1])
        self.assertEqual(plan["input_hashes"]["OwnerFixed\\swarm-runtime.exe"].lower(),
                         "5a50dc83795d74c9b7f54bbd2dc21211b520720e35db8420378b2d612e372fde")
        self.assertEqual((plan["attempts"], plan["retries"], plan["guest_deadline_seconds"]), (1, 0, 420))
        self.assertIn("uninstall", plan["guest_uninstall"].lower())
        self.assertIn("Data", plan["original_data"])
        for field in ("reinstall", "checkpoint_restore", "network_change", "power_fault", "host_os_mutation"):
            self.assertIs(plan[field], False)

    @unittest.skipIf((OUT / "approval.json").exists() or (OUT / "attempt.json").exists(),
                     "Pre-approval check was already recorded; preserve actual authority/attempt")
    def test_no_default_execution_and_missing_approval_refuse_without_writes(self):
        self.assertFalse((OUT / "approval.json").exists(), "Run before real approval only")
        self.assertFalse((OUT / "attempt.json").exists(), "Do not repeat after a real attempt")
        before = sorted(p.name for p in OUT.iterdir())
        self.assertNotEqual(call().returncode, 0)
        missing = call("-ExecuteApproved")
        self.assertNotEqual(missing.returncode, 0)
        self.assertIn("approval.json", missing.stderr.replace("\n", "").replace("\r", ""))
        self.assertEqual(sorted(p.name for p in OUT.iterdir()), before)

    @unittest.skipUnless((OUT / "attempt.json").exists(), "Requires an already consumed attempt")
    def test_consumed_attempt_refuses_before_vm_access(self):
        before = sorted(p.name for p in OUT.iterdir())
        result = call("-ExecuteApproved")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("VM extension consumed", result.stderr)
        self.assertEqual(sorted(p.name for p in OUT.iterdir()), before)


if __name__ == "__main__":
    unittest.main()
