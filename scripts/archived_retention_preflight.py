"""Filesystem-only readiness for the new archived Thread baseline; never starts Swarm."""
from datetime import datetime, timedelta, timezone
import hashlib
import json
from pathlib import Path
import sys
import time

WORK = Path(__file__).resolve().parents[1]
BASE = Path("E:/Swarm-Validation-Host/run-20261023-archived-retention")
ROOT = BASE / "Data"
ARTIFACT = WORK / "target/archived-retention-20261023"
CANDIDATE = WORK / "target/distribution/candidate-20261019-account-feedback"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def assess():
    require(len(sys.argv) == 1, "fixed inputs only; no root/time override")
    for path in (BASE, ROOT, ARTIFACT, CANDIDATE):
        require(path.resolve() == path.absolute(), "redirected fixed path")
    proof = json.loads((ARTIFACT / "after-filesystem.json").read_text(encoding="utf-8"))
    require(proof["normal_process_exit_verified"] is True, "normal closeout not verified")
    require(digest(BASE / "baseline.json") == proof["baseline_sha256"], "baseline changed")
    b = json.loads((BASE / "baseline.json").read_text(encoding="utf-8"))
    require(Path(b["root"]) == ROOT, "different baseline root")
    require(b["elapsed_day_accepted"] is False and b["background_watcher"] is False, "unexpected baseline status")
    require(b["archived_thread"]["archived"] is True, "archived baseline absent")
    require(b["settings"] == {"history_days": None, "deleted_days": None,
            "deleted_board_days": None, "deleted_thread_days": None, "archived_thread_days": 1},
            "different retention settings")
    posts = b["archived_posts"]
    require(len(posts) == 1000 and [p["no"] for p in posts] == list(range(1, 1001)), "incomplete archive baseline")
    require(len({p["id"] for p in posts}) == 1000, "duplicate archived IDs")
    require(posts[0]["revision"] == 2 and posts[1]["reply_to"] == 1, "history/reply baseline missing")
    require(b["ai_released"] is True and b["public_positive_archived_history"] is True
            and b["public_positive_live_history"] is True, "public positive/release missing")
    require(isinstance(b["actor_id"], str) and b["actor_id"], "saved actor ID missing")
    require(digest(ROOT / "RULES.md") == b["rules_sha256"], "fixture RULES changed")
    for name, expected in {
        "swarm-runtime.exe": "0cf3ecb753c20830be7edf2c3630b3cc07aac90655d963b1097374f8b2b24210",
        "swarm.exe": "d23b7973d229bea186d4aec95eca3a6f7f2868f7243534156372e5e438ecfd33",
    }.items():
        require(digest(CANDIDATE / name) == expected, "fixed candidate changed")
    not_before = b["observe_not_before_unix_seconds"]
    require(not_before == b["baseline_unix_seconds"] + 86400, "real-day boundary changed")
    archived_at = datetime.fromisoformat(b["archived_thread"]["archived_at"].replace("Z", "+00:00")).timestamp()
    require(b["baseline_unix_seconds"] >= archived_at, "baseline before archive")
    now = int(time.time())
    ready = now >= not_before
    return {
        "status": "READY_FOR_AUTHORIZED_OBSERVATION" if ready else "WAIT_REAL_ELAPSED_TIME",
        "not_before_jst": datetime.fromtimestamp(not_before, timezone(timedelta(hours=9))).isoformat(),
        "remaining_seconds": max(0, not_before - now),
        "baseline_sha256": proof["baseline_sha256"],
        "runtime_started": False,
        "data_stores_opened": False,
        "fixture_modified": False,
        "background_watcher": False,
        "formal_ac_accepted": False,
    }


if __name__ == "__main__":
    try:
        result = assess()
        print(json.dumps(result, ensure_ascii=True))
        sys.exit(0 if result["status"] == "READY_FOR_AUTHORIZED_OBSERVATION" else 2)
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(json.dumps({"status": "INPUTS_NOT_READY", "error": str(error),
                          "runtime_started": False, "fixture_modified": False}, ensure_ascii=True))
        sys.exit(1)
