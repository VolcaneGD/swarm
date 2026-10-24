"""Read-only gate for the retained elapsed fixture. Never starts a product process."""

import hashlib
import json
from pathlib import Path
import sys
import time

BASE = Path("E:/Swarm-Validation-Host/run-20261019-elapsed-approved-extension-2")
BASELINE_SHA256 = "1d68c9a0106becf65940dd645c758de3a31fc9588d6dc1d5b4de1342d5c4c644"
RUNTIME = (Path(__file__).resolve().parents[1] / "target/distribution/"
           "candidate-20261019-account-feedback/swarm-runtime.exe")
RUNTIME_SHA256 = "0cf3ecb753c20830be7edf2c3630b3cc07aac90655d963b1097374f8b2b24210"
NOT_BEFORE = 1792505888


def evaluate(baseline_bytes, runtime_sha256, closeout, now):
    """No I/O: pin historical inputs before interpreting them or checking time."""
    if hashlib.sha256(baseline_bytes).hexdigest() != BASELINE_SHA256:
        raise ValueError("baseline hash mismatch; preserve files and stop")
    if runtime_sha256 != RUNTIME_SHA256:
        raise ValueError("frozen runtime hash mismatch")
    baseline = json.loads(baseline_bytes.decode("utf-8"))
    if baseline["observe_not_before_unix_seconds"] != NOT_BEFORE:
        raise ValueError("time boundary mismatch")
    if not all(closeout.get(key) is True for key in (
        "preparation_succeeded", "normal_stop_accepted", "runtime_exited"
    )) or closeout.get("retry_allowed") is not False:
        raise ValueError("preparation closeout not confirmed")
    remaining = max(0, NOT_BEFORE - now)
    return {
        "status": "WAIT_REAL_ELAPSED_TIME" if remaining else "PREFLIGHT_READY_ONLY",
        "remaining_seconds": remaining,
        "not_before_jst": "2026-10-20T23:18:08+09:00",
        "baseline_sha256": BASELINE_SHA256,
        "runtime_sha256": runtime_sha256,
        "account_id": baseline["account_id"],
        "live_control": {key: baseline["live_control"][key] for key in
                         ("id", "thread_id", "no", "revision", "body_sha256")},
        "deleted_post_id": baseline["deleted_post"]["id"],
        "deleted_thread_id": baseline["deleted_thread"]["id"],
        "deleted_board_id": baseline["deleted_board"]["id"],
        "required_observations": [
            "same-root native GUI and public AI readback after real elapsed time",
            "live control ID/revision/body hash unchanged",
            "expired history and deleted content purged by product behavior",
            "restore cannot revive purged content; record actual product refusal",
            "normal shutdown and preserve original baseline and all old roots",
        ],
        "archived_thread_case_prepared": False,
        "elapsed_acceptance": "NOT_RUN",
        "product_started": False,
        "reseed_allowed": False,
        "watcher": False,
    }


def main():
    if len(sys.argv) != 1:
        raise ValueError("no root, time override or execute arguments are accepted")
    # Resolve paths only; do not enumerate or open Data stores.
    if BASE.resolve(strict=True) != BASE.absolute():
        raise ValueError("fixed root redirected")
    if (BASE / "Data").resolve(strict=True) != BASE.resolve(strict=True) / "Data":
        raise ValueError("Data redirected outside the fixed root")
    baseline_bytes = (BASE / "baseline.json").read_bytes()
    closeout = json.loads((BASE / "closeout.json").read_text(encoding="utf-8"))
    with RUNTIME.open("rb") as runtime_file:
        digest = hashlib.file_digest(runtime_file, "sha256").hexdigest()
    result = evaluate(baseline_bytes, digest, closeout, int(time.time()))
    print(json.dumps(result, ensure_ascii=True, indent=2))
    return 2 if result["remaining_seconds"] else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, KeyError) as error:
        print(json.dumps({"status": "REFUSED", "reason": str(error)}))
        sys.exit(1)
