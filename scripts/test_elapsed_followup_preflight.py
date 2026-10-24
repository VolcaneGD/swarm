"""Exercise the gate with retained metadata, without product/store access."""

import unittest

import elapsed_followup_preflight as gate


@unittest.skipUnless((gate.BASE / "baseline.json").is_file(), "retained fixture required")
class FollowupGateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.baseline = (gate.BASE / "baseline.json").read_bytes()
        cls.closeout = {key: True for key in (
            "preparation_succeeded", "normal_stop_accepted", "runtime_exited")}
        cls.closeout["retry_allowed"] = False

    def evaluate(self, now, **kwargs):
        return gate.evaluate(kwargs.get("baseline", self.baseline),
                             kwargs.get("runtime", gate.RUNTIME_SHA256),
                             kwargs.get("closeout", self.closeout), now)

    def test_real_boundary_does_not_claim_acceptance(self):
        before = self.evaluate(gate.NOT_BEFORE - 1)
        self.assertEqual(before["status"], "WAIT_REAL_ELAPSED_TIME")
        self.assertEqual(before["remaining_seconds"], 1)
        ready = self.evaluate(gate.NOT_BEFORE)
        self.assertEqual(ready["status"], "PREFLIGHT_READY_ONLY")
        self.assertEqual(ready["elapsed_acceptance"], "NOT_RUN")
        self.assertFalse(ready["product_started"])

    def test_tampered_baseline_refused_even_after_boundary(self):
        with self.assertRaisesRegex(ValueError, "baseline hash"):
            self.evaluate(gate.NOT_BEFORE + 1, baseline=self.baseline + b" ")

    def test_replaced_candidate_refused(self):
        with self.assertRaisesRegex(ValueError, "runtime hash"):
            self.evaluate(gate.NOT_BEFORE, runtime="0" * 64)

    def test_failed_closeout_refused(self):
        with self.assertRaisesRegex(ValueError, "closeout"):
            self.evaluate(gate.NOT_BEFORE,
                          closeout=dict(self.closeout, runtime_exited=False))


if __name__ == "__main__":
    unittest.main()
