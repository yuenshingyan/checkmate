#!/usr/bin/env python3
"""Live LSP regression (stdlib only): python3 scripts/test_rust_analyzer.py PATH_TO_RA.

Uses the repository's toolchain and .vscode/settings.json, including server.extraEnv.
No source/baseline files are written. Cargo/RA may populate target/. The negative
control changes only an LSP buffer, then restores it: on-disk token agreement must
still be enforced. Enables experimental native diagnostics (never disables any),
and pulls diagnostics after edits: this RA version does not always push them.
Settings must be strict JSON, not JSONC. Requires a compatible rust-analyzer/
proc-macro server pair; this is an editor regression, not a cargo-test replacement.
"""
import argparse
from collections import deque
import json
import os
from pathlib import Path
import queue
import re
import signal
import subprocess
import sys
import threading
import time

ROOT = Path(__file__).resolve().parents[1]


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def position(text, needle, occurrence=0):
    matches = list(re.finditer(re.escape(needle), text))
    require(len(matches) > occurrence, f"missing source anchor: {needle}")
    prefix = text[:matches[occurrence].start()]
    return {"line": prefix.count("\n"),
            "character": len(prefix.rsplit("\n", 1)[-1].encode("utf-16-le")) // 2}


def compact(text):
    return re.sub(r",([)\]}])", r"\1", re.sub(r"\s+", "", text))


def expansion_code(source, aliases):
    # Exclude embedded source/snapshot strings before resolving generated imports.
    code = re.sub(r'"(?:\\.|[^"\\])*"', '""', source)
    # Both `use ::std` and `use::std` are valid Rust/RA pretty-printing.
    for route, alias in re.findall(r"\buse\b\s*([\w\s:]+)\s+as\s+(\w+)\s*;", code):
        aliases[alias] = route.strip().split("::")[-1].strip()
    for alias, name in aliases.items():
        code = re.sub(r"\b" + re.escape(alias) + r"\b", name, code)
    return code


class LSP:
    def __init__(self, binary, timeout):
        self.timeout = timeout
        self.messages = queue.Queue(maxsize=4096)
        self.log = deque(maxlen=40)
        self.diagnostics = {}
        self.serial = 0
        self.ready = False
        self.config = {}
        settings = ROOT / ".vscode/settings.json"
        if settings.exists():
            # Deliberately fail on unsupported JSONC instead of silently ignoring config.
            for key, value in json.loads(settings.read_text()).items():
                if key.startswith("rust-analyzer."):
                    parts = key[len("rust-analyzer."):].split(".")
                    target = self.config
                    for part in parts[:-1]:
                        target = target.setdefault(part, {})
                    target[parts[-1]] = value
        # RA's native proc-macro-error diagnostic is experimental; enabling it
        # is essential for unsaved-buffer errors (Cargo only sees disk).
        self.config.setdefault("diagnostics", {}).setdefault("experimental", {})["enable"] = True
        env = os.environ.copy()
        env.update(self.config.get("server", {}).get("extraEnv", {}))
        self.process = subprocess.Popen(
            [binary], cwd=ROOT, env=env, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        # A whole-session watchdog also bounds synchronous pipe writes and
        # shutdown if a wedged server stops reading, not just response waits.
        self.watchdog = threading.Timer(4 * timeout, self.kill_session)
        self.watchdog.daemon = True
        self.watchdog.start()
        threading.Thread(target=self.reader, daemon=True).start()
        threading.Thread(target=self.stderr, daemon=True).start()

    def kill_session(self):
        self.log.append("whole-session LSP deadline exceeded")
        try:
            os.killpg(self.process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass

    def stderr(self):
        for line in self.process.stderr:
            self.log.append(line.decode(errors="replace")[-2000:].rstrip())

    def reader(self):
        try:
            while True:
                headers = {}
                while True:
                    line = self.process.stdout.readline()
                    if not line:
                        raise EOFError("rust-analyzer stdout closed")
                    if line == b"\r\n":
                        break
                    key, value = line.decode().split(":", 1)
                    headers[key.lower()] = value.strip()
                size = int(headers["content-length"])
                require(size <= 16 * 1024 * 1024, "oversized LSP message")
                body = self.process.stdout.read(size)
                self.messages.put(json.loads(body))
        except Exception as error:
            self.messages.put(error)

    def send(self, message):
        body = json.dumps({"jsonrpc": "2.0", **message}).encode()
        self.process.stdin.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
        self.process.stdin.flush()

    def notify(self, method, params):
        self.send({"method": method, "params": params})

    def pump(self, deadline):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("LSP deadline exceeded")
        try:
            message = self.messages.get(timeout=remaining)
        except queue.Empty:
            raise TimeoutError("waiting for rust-analyzer") from None
        if isinstance(message, Exception):
            raise message
        method, params = message.get("method"), message.get("params", {})
        if method and "id" in message:
            if method == "workspace/configuration":
                result = []
                for item in params["items"]:
                    section = item.get("section", "rust-analyzer")
                    value = self.config
                    for part in section.removeprefix("rust-analyzer").strip(".").split("."):
                        if part:
                            value = value.get(part) if isinstance(value, dict) else None
                    result.append(value)
            elif method in ("client/registerCapability", "client/unregisterCapability",
                            "window/workDoneProgress/create", "workspace/diagnostic/refresh"):
                result = None
            elif method == "workspace/workspaceFolders":
                result = [{"uri": ROOT.as_uri(), "name": ROOT.name}]
            else:
                self.send({"id": message["id"], "error": {
                    "code": -32601, "message": f"unsupported client request {method}"}})
                return message
            self.send({"id": message["id"], "result": result})
        elif method == "experimental/serverStatus":
            self.ready = params.get("quiescent", False)
            self.log.append(str(params))
        elif method == "textDocument/publishDiagnostics":
            self.diagnostics[params["uri"]] = params
        elif method in ("window/logMessage", "window/showMessage"):
            self.log.append(str(params)[-2000:])
        return message

    def request(self, method, params):
        self.serial += 1
        ident = self.serial
        self.send({"id": ident, "method": method, "params": params})
        deadline = time.monotonic() + self.timeout
        while True:
            message = self.pump(deadline)
            if message.get("id") == ident and "method" not in message:
                require("error" not in message, f"{method}: {message.get('error')}")
                return message.get("result")

    def wait(self, predicate):
        deadline = time.monotonic() + self.timeout
        while not predicate():
            self.pump(deadline)

    def start(self):
        self.request("initialize", {
            "processId": os.getpid(), "rootUri": ROOT.as_uri(),
            "workspaceFolders": [{"uri": ROOT.as_uri(), "name": ROOT.name}],
            "capabilities": {"workspace": {"configuration": True},
                             "window": {"workDoneProgress": True},
                             "experimental": {"serverStatusNotification": True}},
            "initializationOptions": self.config})
        self.notify("initialized", {})
        self.wait(lambda: self.ready)
        print("rust-analyzer ready (workspace loaded)")

    def close(self):
        # Kill the whole session, including Cargo/proc-macro children, even on timeout.
        if self.process.poll() is None:
            try:
                self.timeout = min(self.timeout, 5)
                self.request("shutdown", None)
                self.notify("exit", None)
                self.process.wait(timeout=5)
            except Exception:
                pass
        try:
            os.killpg(self.process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            self.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            pass
        try:
            os.killpg(self.process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        self.process.wait(timeout=3)
        self.watchdog.cancel()


def run(client):
    client.start()
    documents = {}
    for example, file in [("struct-return", "automatic"), ("local-types", "automatic"),
                          ("local-types", "context_snapshot"),
                          ("inherent-methods", "automatic"), ("automatic-context", "automatic")]:
        path = ROOT / "examples" / example / "tests" / (file + ".rs")
        text, uri = path.read_text(), path.as_uri()
        documents[example + "/" + file] = (uri, text)
        client.notify("textDocument/didOpen", {"textDocument": {
            "uri": uri, "languageId": "rust", "version": 1, "text": text}})

    def query(key, method, anchor, occurrence=0):
        uri, text = documents[key]
        return client.request(method, {"textDocument": {"uri": uri},
                                       "position": position(text, anchor, occurrence)})

    aliases = {}

    def expansion(key, fragments, occurrence=0):
        result = query(key, "rust-analyzer/expandMacro", "extract_fn!", occurrence)
        source = (result or {}).get("expansion", "")
        require(source and "compile_error!" not in source, f"{key}: invalid expansion: {source}")
        code = expansion_code(source, aliases)
        for fragment in fragments:
            require(compact(fragment) in compact(code), f"{key}: missing {fragment!r} in {code[:6000]}")
        print(f"PASS expansion {key} #{occurrence + 1}")
        return source

    def hover(key, anchor, fragments):
        result = query(key, "textDocument/hover", anchor)
        contents = (result or {}).get("contents", {})
        rendered = json.dumps(contents, ensure_ascii=False) if not isinstance(contents, dict) else contents.get("value", "")
        for alias, name in aliases.items():
            rendered = re.sub(r"\b" + alias + r"\b", name, rendered)
        require(rendered and "{unknown}" not in rendered, f"{key}: unknown hover {anchor}: {rendered}")
        for fragment in fragments:
            require(compact(fragment) in compact(rendered), f"{key}: hover {anchor} missing {fragment!r}: {rendered}")
        print(f"PASS hover {key}: {anchor}: {', '.join(fragments)}")

    expansion("struct-return/automatic", ["fn automatic_user", "checkmate_struct_example::models::User"])
    expansion("local-types/automatic", ["fn build_user", "name: String", "active: bool"])
    expansion("local-types/context_snapshot", ["fn snapshot_user", "fn user_context", "-> &'static ::core::primitive::str"])
    expansion("inherent-methods/automatic", ["fn finish_counter", "fn finish", "-> u8"])
    expansion("automatic-context/automatic", ["fn extracted_invoice", "checkmate_auto_example::Invoice"])
    expansion("automatic-context/automatic", ["fn invoice_fields", "-> (u64, u32)"], 1)
    hover("struct-return/automatic", 'automatic_user("Bob"', ["fn automatic_user", "name: String", "active: bool", "-> User"])
    hover("struct-return/automatic", 'name, "Bob"', ["name: String"])
    hover("struct-return/automatic", 'active);', ["active: bool"])
    hover("local-types/automatic", 'build_user("Bob"', ["fn build_user", "-> Checkmate"])
    hover("local-types/automatic", 'name, "Bob"', ["name: String"])
    hover("local-types/automatic", 'active);', ["active: bool"])
    hover("inherent-methods/automatic", 'finish_counter(n)', ["fn finish_counter", "-> u8"])
    hover("local-types/context_snapshot", 'user_context());', ["fn user_context", "-> &'static ::core::primitive::str"])
    hover("automatic-context/automatic", 'invoice_context()', ["fn invoice_context", "-> &'static ::core::primitive::str"])

    # Pull fresh reports after expansion/hover; initial pushed reports can
    # predate macro loading and are not sufficient evidence of clean analysis.
    for uri, _ in documents.values():
        report = client.request("textDocument/diagnostic", {"textDocument": {"uri": uri}})
        require(report and report.get("kind") == "full", f"missing positive diagnostics: {uri}")
        client.diagnostics[uri] = {"diagnostics": report["items"]}
    def clean():
        errors = [(uri, d) for uri, report in client.diagnostics.items()
                  for d in report["diagnostics"] if d.get("severity") == 1]
        require(not errors, f"unexpected workspace error diagnostics: {errors}")
    clean()
    print("PASS positive diagnostics (no errors; diagnostics enabled)")

    key = "struct-return/automatic"
    uri, original = documents[key]
    changed = original.replace("output = user,", "output = (user),", 1)
    require(changed != original, "negative edit did not change tokens")
    def change(text, version):
        client.diagnostics.pop(uri, None)
        client.notify("textDocument/didChange", {"textDocument": {"uri": uri, "version": version},
                                               "contentChanges": [{"text": text}]})
    try:
        change(changed, 2)
        result = query(key, "rust-analyzer/expandMacro", "extract_fn!")
        source = (result or {}).get("expansion", "")
        require("Checkmate" in source and "tokens do not match" in source,
                f"unsaved token mismatch was not rejected: {source}")
        report = client.request("textDocument/diagnostic", {"textDocument": {"uri": uri}})
        require(report and report.get("kind") == "full", "missing negative diagnostics")
        client.diagnostics[uri] = {"diagnostics": report["items"]}
        client.wait(lambda: any("Checkmate" in d["message"] and "tokens do not match" in d["message"]
                               and d.get("severity") == 1
                               for d in client.diagnostics.get(uri, {}).get("diagnostics", [])))
        print("PASS negative: unsaved token mismatch produces Checkmate error diagnostic")
    except Exception:
        print("Negative diagnostics: " + json.dumps(client.diagnostics.get(uri)), file=sys.stderr)
        raise
    finally:
        change(original, 3)
    expansion(key, ["fn automatic_user", "checkmate_struct_example::models::User"])
    report = client.request("textDocument/diagnostic", {"textDocument": {"uri": uri}})
    require(report and report.get("kind") == "full", "missing restored diagnostics")
    client.diagnostics[uri] = {"diagnostics": report["items"]}
    clean()
    require((ROOT / "examples/struct-return/tests/automatic.rs").read_text() == original,
            "source changed on disk")
    print("PASS restored buffer: expansion and diagnostics recovered")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("rust_analyzer", help="path to the real rust-analyzer binary")
    parser.add_argument("--timeout", type=float, default=180, help="seconds per LSP operation (default: 180)")
    args = parser.parse_args()
    require(args.timeout > 0, "timeout must be positive")
    require(os.name == "posix", "this harness requires POSIX process-group cleanup")
    client = None
    try:
        client = LSP(str(Path(args.rust_analyzer).resolve()), args.timeout)
        run(client)
        return 0
    except Exception as error:
        print(f"FAIL: {error}", file=sys.stderr)
        if client:
            print("Diagnostics: " + json.dumps(client.diagnostics)[-12000:], file=sys.stderr)
            print("Recent server log:\n" + "\n".join(client.log), file=sys.stderr)
        return 1
    finally:
        if client:
            client.close()


if __name__ == "__main__":
    sys.exit(main())
