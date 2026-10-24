"""Measure tool-to-tool handoff only; never opens Swarm stores or pipes."""
import json
import time
from pathlib import Path

root = Path(__file__).resolve().parents[3]
out = root / "target" / "native-drain-timing-20261019"
out.mkdir(exist_ok=False)
started = time.monotonic()
with (out / "ready.json").open("x", encoding="utf-8") as file:
    json.dump({"product_operation": False, "hold_seconds": 4}, file)
while not (out / "signal").exists() and time.monotonic() - started < 4:
    time.sleep(0.005)
elapsed = time.monotonic() - started
with (out / "result.json").open("x", encoding="utf-8") as file:
    json.dump({"signal_within_bound": (out / "signal").exists(),
               "tool_handoff_ms": round(elapsed * 1000),
               "product_operation": False, "native_input": False}, file)
