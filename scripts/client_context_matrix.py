"""Bounded, opt-in real Codex/Gemini context checks in an exclusive fixture.

No installs, global configuration writes, production roots, or automatic retries.
The fixture helper is a separately reviewed developer-only binary. Native clients
receive only this test procedure, never Swarm conversation bodies or credentials.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import uuid

WORK = Path(__file__).resolve().parents[1]
FLAGS = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0


def save(path, value):
    path.write_text(json.dumps(value, indent=2), encoding="utf-8")


def wait(path, seconds=30):
    deadline = time.monotonic() + seconds
    while not path.exists():
        if time.monotonic() >= deadline:
            raise TimeoutError("barrier " + path.name)
        time.sleep(.1)
    return json.loads(path.read_text(encoding="utf-8"))


def cli(run, alias, args, exits=(0,)):
    ready = wait(run / "ready.json")
    env = os.environ.copy()
    for key in ("SWARM_PROJECT_ID", "SWARM_SESSION"):
        env.pop(key, None)
    result = subprocess.run([
        str(WORK / "target/debug/swarm.exe"), "--data-dir", ready["data_root"],
        "--format", "json", "--session", alias, *args,
    ], capture_output=True, timeout=20, creationflags=FLAGS, env=env)
    value = json.loads(result.stdout)
    assert result.returncode in exits and value["ok"] == (result.returncode == 0), (
        args[:2], result.returncode, (value.get("error") or {}).get("code"))
    return value.get("data"), {"operation": args[:2], "exit": result.returncode,
                                "error": (value.get("error") or {}).get("code")}


def capture_hook(run):
    value = json.load(sys.stdin)
    selector = str(uuid.UUID(value["session_id"]))
    save(run / ("gemini-" + selector + ".json"), {
        "session_id": selector, "cwd": value["cwd"],
        "hook_event_name": value["hook_event_name"],
        "llm_request": {"model": value["llm_request"]["model"]},
    })
    print("{}")


def inspect_run(run):
    """Read existing receipts/logs without launching a client or product process."""
    ready = wait(run / "ready.json", 1)
    closeout = wait(run / "closeout.json", 1)
    stopped = wait(run / "stopped.json", 1)
    assert closeout["status"] == "PASS" and closeout["fixture_exit"] == 0
    assert stopped["human_stop_accepted"] and stopped["graceful_exit"]
    assert len(closeout["clients"]) == 6
    selectors, identities, projects = set(), set(), set()
    provenance = []
    for owned in closeout["clients"]:
        label = owned["label"]
        assert owned["exit"] == 0
        task = wait(run / (label + "-task.json"), 1)
        receipt = wait(run / (label + "-verification.json"), 1)
        assert receipt["status"] == "PASS" and len(receipt["checks"]) == 6
        assert [item["exit"] for item in receipt["checks"]] in ([0, 0, 2, 3, 2, 0], [0, 0, 6, 3, 2, 0])
        assert receipt["model"] == task["model"]
        assert receipt["project_id"] == ready["projects"][task["project_index"]]["id"]
        selectors.add(receipt["native_session_id"])
        identities.add(receipt["identity_id"])
        projects.add(receipt["project_id"])
        events = [json.loads(line) for line in (run / (label + ".stdout.log")).read_text(
            encoding="utf-8").splitlines() if line.startswith("{")]
        if task["client"] == "codex":
            matching = [event["item"] for event in events if event.get("type") == "item.completed"
                and event.get("item", {}).get("type") == "command_execution"
                and "client_context_matrix.py" in event["item"].get("command", "")
                and "driver" in event["item"]["command"] and label in event["item"]["command"]]
            assert matching and all(item["exit_code"] == 0 for item in matching)
        else:
            uses = [event for event in events if event.get("type") == "tool_use"
                and event.get("tool_name") == "run_shell_command"
                and "client_context_matrix.py" in event.get("parameters", {}).get("command", "")
                and "driver" in event["parameters"]["command"] and label in event["parameters"]["command"]]
            assert uses
            for use in uses:
                assert any(event.get("type") == "tool_result" and event.get("tool_id") == use["tool_id"]
                           and event.get("status") == "success" for event in events)
        provenance.append({"label": label, "actual_terminal_completed": True})
    assert len(selectors) == len(identities) == 6 and len(projects) == 2
    return {"status": "PASS", "actual_client_checks": 36, "distinct_native_sessions": 6,
            "distinct_swarm_ids": 6, "registered_projects": 2, "terminal_provenance": provenance,
            "read_only_inspection": True, "historical_signature_claim": False}


def driver(run, label):
    assert label in {client + suffix for client in ("codex", "gemini")
                     for suffix in ("-a1", "-a2", "-b")}
    task = wait(run / (label + "-task.json"))
    ready = wait(run / "ready.json")
    project = ready["projects"][task["project_index"]]
    other = ready["projects"][1 - task["project_index"]]
    assert Path.cwd().resolve() == Path(project["root"]).resolve()
    selector = wait(run / (label + "-session.json"))["id"]
    if task["client"] == "codex":
        sources = list((Path.home() / ".codex/sessions").glob("*/*/*/rollout-*" + selector + ".jsonl"))
        assert len(sources) == 1, "exact native rollout required"
        source = sources[0]
        assert source.stat().st_size <= 64 * 1024 * 1024
        digest = hashlib.sha256()
        rows = []
        with source.open("rb") as stream:
            for line in stream:
                assert len(line) <= 1024 * 1024
                digest.update(line)
                event = json.loads(line)
                if event.get("type") in ("session_meta", "turn_context"):
                    payload = event["payload"]
                    assert Path(payload["cwd"]).resolve() == Path(project["root"]).resolve()
                    fields = ("id", "cwd") if event["type"] == "session_meta" else ("cwd", "model")
                    rows.append({"type": event["type"], "payload": {
                        k: payload[k] for k in fields if k in payload}})
        assert rows[0]["payload"]["id"] == selector
        model = rows[-1]["payload"]["model"]
        report = run / (label + "-metadata.jsonl")
        report.write_text("\n".join(json.dumps(row) for row in rows) + "\n", encoding="utf-8")
        save(run / (label + "-source.json"), {
            "path": str(source), "capture_sha256": digest.hexdigest(), "metadata_only": True})
        client = "codex"
    else:
        report = run / ("gemini-" + selector + ".json")
        hook = wait(report)
        assert hook["session_id"] == selector
        assert Path(hook["cwd"]).resolve() == Path(project["root"]).resolve()
        model = hook["llm_request"]["model"]
        client = "gemini-cli"
    assert model == task["model"], "requested and observed selection differ"
    args = ["--model-client", client, "--model-session-id", selector,
            "--model-report-file", str(report)]
    records = []

    def check(arguments, exits=(0,)):
        value, receipt = cli(run, label, arguments, exits)
        records.append(receipt)
        return value

    identity = check(["identity", "start", "--project-id", project["id"], *args])["identity"]
    assert identity["model_name"] == model and identity["model_source"] == "automatic"
    save(run / (label + "-started.json"), {"identity": identity, "selector": selector,
        "project_id": project["id"], "report": str(report)})
    wait(run / "all-started.json", 180)
    before = check(["identity", "status"])
    assert before["id"] == identity["id"]
    check(["identity", "model", *args, "--model-project-root", other["root"]], (2, 6))
    check(["--project", other["id"], "identity", "status"], (3,))
    peer_label = task["client"] + ("-a2" if label.endswith("-a1") else "-a1")
    peer = wait(run / (peer_label + "-started.json"))
    assert peer["selector"] != selector and peer["identity"]["id"] != identity["id"]
    foreign = args.copy()
    foreign[foreign.index("--model-session-id") + 1] = peer["selector"]
    check(["identity", "model", *foreign], (2,))
    assert check(["identity", "status"]) == before
    save(run / (label + "-verification.json"), {
        "status": "PASS", "native_session_id": selector, "project_id": project["id"],
        "model": model, "identity_id": identity["id"], "checks": records,
        "execution_subject": "real native client terminal driver"})
    print(json.dumps({"status": "PASS", "subject": label, "checks": len(records)}))


def supervise(run, helper):
    assert not run.exists(), "run path must be exclusive; never reuses an incident"
    assert helper == (WORK / "target/debug/swarm-client-context-fixture.exe").resolve()
    assert helper.is_file(), "build the reviewed scripts/fixtures/client-context helper first"
    node = shutil.which("node")
    codex = Path(os.environ["APPDATA"]) / "npm/node_modules/@openai/codex/bin/codex.js"
    gemini = Path(os.environ["APPDATA"]) / "npm/node_modules/@google/gemini-cli/bundle/gemini.js"
    assert node and codex.is_file() and gemini.is_file()
    fixture = subprocess.Popen([str(helper), str(run), str(WORK)], creationflags=FLAGS)
    owned = []
    status = "FAILED"
    try:
        ready = wait(run / "ready.json", 25)
        for project in ready["projects"]:
            config = Path(project["root"]) / ".gemini"
            config.mkdir()
            command = subprocess.list2cmdline([sys.executable, str(Path(__file__).resolve()), "hook", "--run", str(run)])
            save(config / "settings.json", {"hooks": {"BeforeModel": [{"matcher": "*", "hooks": [
                {"type": "command", "name": "swarm-context-metadata", "command": command}]}]},
                "telemetry": {"enabled": False}})
        deadline = time.monotonic() + 240
        for client, model in (("codex", "gpt-5.6-luna"), ("gemini", "gemini-2.5-flash")):
            for suffix, project_index in (("a1", 0), ("a2", 0), ("b", 1)):
                label = client + "-" + suffix
                project = ready["projects"][project_index]
                save(run / (label + "-task.json"), {"client": client, "model": model, "project_index": project_index})
                command = subprocess.list2cmdline([sys.executable, str(Path(__file__).resolve()),
                    "driver", "--run", str(run), "--label", label])
                prompt = (f"Authorized bounded Swarm context acceptance test. Read {WORK.as_posix()}/.agents/skills/swarm-cli/SKILL.md "
                    f"and {WORK.as_posix()}/docs/AI_USAGE.md, then execute exactly this terminal command: {command}\n"
                    "The driver uses a new isolated developer fixture and only your exact metadata fields. It checks session/project separation. "
                    "Return actual PASS/FAIL. Do not edit files, install/update, change settings, invoke agents or other tests, "
                    "touch other data/jobs, or expose credentials or conversation bodies. Finish after the command.")
                if client == "codex":
                    cmd = [node, str(codex), "exec", "--model", model, "-c", "mcp_servers.openviking.enabled=false",
                        "-c", "mcp_servers.serena.enabled=false", "--skip-git-repo-check", "--sandbox", "danger-full-access",
                        "--add-dir", str(WORK), "--cd", project["root"], "--json", prompt]
                else:
                    selector = str(uuid.uuid4())
                    save(run / (label + "-session.json"), {"id": selector})
                    cmd = [node, str(gemini), "--model", model, "--session-id", selector, "--skip-trust",
                        "--approval-mode", "auto_edit", "--allowed-tools", "run_shell_command", "--allowed-tools", "read_file",
                        "--allowed-mcp-server-names", "swarm-no-mcp", "--extensions", "none", "--include-directories", str(WORK),
                        "--output-format", "stream-json", "--prompt", prompt]
                out = (run / (label + ".stdout.log")).open("wb")
                err = (run / (label + ".stderr.log")).open("wb")
                env = os.environ.copy()
                env.update(CI="1", NO_COLOR="1")
                proc = subprocess.Popen(cmd, cwd=project["root"], env=env, stdout=out, stderr=err, creationflags=FLAGS)
                owned.append((label, proc, out, err))
                save(run / (label + "-owner.json"), {"pid": proc.pid, "project_root": project["root"]})
        while True:
            assert time.monotonic() < deadline, "240 second native invocation deadline"
            for label, proc, _, _ in owned:
                if label.startswith("codex") and not (run / (label + "-session.json")).exists():
                    for line in (run / (label + ".stdout.log")).read_text(encoding="utf-8", errors="replace").splitlines():
                        try:
                            event = json.loads(line)
                        except json.JSONDecodeError:
                            continue
                        if event.get("type") == "thread.started":
                            save(run / (label + "-session.json"), {"id": event["thread_id"]})
                            break
                if proc.poll() is not None:
                    assert proc.returncode == 0 and (run / (label + "-verification.json")).exists(), label + " exited without PASS"
            if not (run / "all-started.json").exists() and all((run / (label + "-started.json")).exists() for label, *_ in owned):
                started = [wait(run / (label + "-started.json")) for label, *_ in owned]
                assert len({x["identity"]["id"] for x in started}) == 6
                assert len({x["selector"] for x in started}) == 6
                save(run / "all-started.json", {"distinct_native_sessions": 6, "distinct_swarm_ids": 6, "projects": 2})
            if all(proc.poll() is not None for _, proc, *_ in owned):
                break
            time.sleep(.2)
        reports = [wait(run / (label + "-verification.json"), 1) for label, *_ in owned]
        assert all(item["status"] == "PASS" for item in reports)
        # Release one session while the other protected aliases remain active.
        isolation = []
        for client in ("codex", "gemini"):
            _, receipt = cli(run, client + "-a1", ["identity", "release"])
            isolation.append(receipt)
            for suffix in ("a2", "b"):
                identity, receipt = cli(run, client + "-" + suffix, ["identity", "status"])
                expected = wait(run / (client + "-" + suffix + "-verification.json"))["identity_id"]
                assert identity["id"] == expected
                isolation.append(receipt)
        status = "PASS"
        save(run / "verification.json", {"status": status, "clients": len(reports),
            "actual_client_checks": sum(len(item["checks"]) for item in reports), "supervisor_release_isolation": isolation})
    except Exception as error:
        if run.exists():
            save(run / "error.json", {"type": type(error).__name__, "error": str(error)})
        print(json.dumps({"status": "FAILED", "type": type(error).__name__, "error": str(error)}), flush=True)
    finally:
        for _, proc, out, err in owned:
            if proc.poll() is None:
                subprocess.run(["taskkill", "/PID", str(proc.pid), "/T", "/F"], capture_output=True, timeout=15)
            out.close()
            err.close()
        cleanup = []
        if (run / "ready.json").exists():
            for label, *_ in owned:
                try:
                    _, receipt = cli(run, label, ["identity", "release"], (0, 3))
                    cleanup.append(receipt)
                except Exception as error:
                    cleanup.append({"label": label, "error_type": type(error).__name__})
            (run / "stop.request").touch(exist_ok=False)
        fixture.wait(timeout=30)
        if run.exists():
            stopped = wait(run / "stopped.json", 1)
            if fixture.returncode != 0 or not stopped.get("human_stop_accepted") or not stopped.get("graceful_exit"):
                status = "FAILED"
            if any("error_type" in receipt for receipt in cleanup):
                status = "FAILED"
            save(run / "closeout.json", {"status": status, "fixture_exit": fixture.returncode,
                "clients": [{"label": label, "pid": proc.pid, "exit": proc.returncode} for label, proc, *_ in owned],
                "release": cleanup})
    print(json.dumps({"status": status, "actual_clients": len(owned),
                      "closeout": str(run / "closeout.json")}), flush=True)
    return 0 if status == "PASS" else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("run", "inspect", "driver", "hook"))
    parser.add_argument("--run", required=True, type=Path)
    parser.add_argument("--helper", type=Path)
    parser.add_argument("--label")
    args = parser.parse_args()
    run = args.run.resolve()
    assert WORK / "target" in run.parents, "test output must be below this workspace target"
    if args.mode == "inspect":
        print(json.dumps(inspect_run(run), indent=2))
    elif args.mode == "hook":
        capture_hook(run)
    elif args.mode == "driver":
        driver(run, args.label)
    else:
        assert args.helper is not None
        sys.exit(supervise(run, args.helper.resolve()))
