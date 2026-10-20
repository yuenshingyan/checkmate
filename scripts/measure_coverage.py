#!/usr/bin/env python3
"""Isolated, stable LLVM line coverage; standard library only.

Full exports may contain dependency mappings. Published totals are restricted to
archived workspace files, and library totals use a file-based src filter.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile


def select_objects(records, package_ids):
    objects = set()
    for record in records:
        if record.get("reason") != "compiler-artifact" or record.get("package_id") not in package_ids:
            continue
        if record.get("executable"):
            objects.add(record["executable"])
        if "proc-macro" in record.get("target", {}).get("kind", []):
            objects.update(p for p in record.get("filenames", [])
                           if Path(p).suffix in {".dylib", ".dll", ".so"})
    return sorted(objects)


def classify_file(filename, source, package_dirs):
    try:
        relative = Path(filename).resolve().relative_to(Path(source).resolve())
    except ValueError:
        return None, "outside archive"
    name = relative.as_posix()
    if not Path(filename).is_file():
        return name, "not an archived file"
    for directory in package_dirs:
        try:
            local = relative.relative_to(directory)
        except ValueError:
            continue
        parts = local.parts
        if "tests" in relative.parts or "examples" in relative.parts:
            return name, "tests/examples directory"
        if (local.name in {"tests.rs", "walkthroughs.rs"} or local.name.endswith("_tests.rs")
                or local.as_posix() == "src/diagnostics/integration.rs"):
            return name, "test file"
        if local.as_posix().endswith("resolve/graph/properties.rs"):
            return name, "generated property tests"
        if parts and parts[0] == "src" and local.suffix == ".rs":
            return name, "library"
        return name, "workspace non-library"
    return name, "outside workspace packages"


def classify_export(exported, source, package_dirs):
    workspace, library, classification = [], [], []
    for data in exported["data"]:
        for item in data["files"]:
            relative, reason = classify_file(item["filename"], source, package_dirs)
            included = relative is not None and reason not in {"outside workspace packages", "not an archived file"}
            classification.append({"path": relative or item["filename"], "workspace": included,
                                   "library": reason == "library", "reason": reason})
            if included:
                workspace.append(item)
            if reason == "library":
                library.append(item)
    return workspace, library, classification


def aggregate(files):
    """Sum LLVM per-file summary.lines, never physical lines or segments."""
    if not files:
        raise ValueError("no mapped files in selected scope")
    seen = {}
    for item in files:
        lines = item["summary"]["lines"]
        count, covered = lines["count"], lines["covered"]
        if not isinstance(count, int) or not isinstance(covered, int) or not 0 <= covered <= count:
            raise ValueError("invalid LLVM line counts")
        name = item["filename"]
        value = (count, covered)
        if name in seen and seen[name] != value:
            raise ValueError("conflicting duplicate file summaries: " + name)
        seen[name] = value
    count = sum(v[0] for v in seen.values())
    covered = sum(v[1] for v in seen.values())
    return {"count": count, "covered": covered,
            "percent": round(100 * covered / count, 4) if count else None,
            "files": len(seen)}


def test_counts(text):
    results = re.findall(r"test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out", text)
    totals = dict.fromkeys(["passed", "failed", "ignored", "measured", "filtered_out"], 0)
    for result in results:
        if result[0] != "ok":
            raise ValueError("unsuccessful test result")
        for key, value in zip(totals, result[1:]):
            totals[key] += int(value)
    if not results:
        raise ValueError("no test result summaries found")
    return {**totals, "result_summaries": len(results)}


def inventory(root):
    result = {}
    for path in sorted(root.rglob("*")):
        stat = path.lstat()
        entry = {"mode": stat.st_mode}
        if path.is_symlink():
            entry.update(kind="symlink", target=os.readlink(path))
        elif path.is_file():
            entry.update(kind="file", size=stat.st_size,
                         sha256=hashlib.sha256(path.read_bytes()).hexdigest())
        elif path.is_dir():
            entry.update(kind="directory")
        else:
            raise ValueError("unexpected source entry: " + str(path))
        result[path.relative_to(root).as_posix()] = entry
    return result


def dump(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--toolchain", required=True)
    args = parser.parse_args()
    checkout = Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip()).resolve()
    output = args.output.resolve()
    if output == checkout or checkout in output.parents:
        parser.error("output must be outside checkout")
    # Reject rather than silently reinterpret user compiler configuration.
    conflicts = [k for k in os.environ if (k in {
        "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS", "CARGO_BUILD_RUSTC",
        "CARGO_BUILD_RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
        "RUSTDOC", "RUSTDOCFLAGS", "CARGO_ENCODED_RUSTDOCFLAGS"}
        or (k.startswith("CARGO_TARGET_") and k.endswith("_RUSTFLAGS"))) and os.environ[k]]
    if conflicts:
        parser.error("conflicting environment: " + ", ".join(sorted(conflicts)))
    revision = subprocess.check_output(["git", "rev-parse", "--verify", "--end-of-options", args.revision + "^{commit}"], text=True).strip()
    output.mkdir(parents=True, exist_ok=False)
    source = output / "source"
    source.mkdir()
    for name in ("profiles", "target", "reports"):
        (output / name).mkdir()
    env = os.environ.copy()
    overrides = {"RUSTUP_TOOLCHAIN": args.toolchain, "CARGO_TARGET_DIR": str(output / "target"),
                 "RUSTFLAGS": "-C instrument-coverage",
                 "LLVM_PROFILE_FILE": str(output / "profiles" / "%p-%m.profraw")}
    env.update(overrides)
    commands = []

    def run(argv, label, cwd=source):
        commands.append({"argv": list(map(str, argv)), "cwd": str(cwd), "stdout": label + ".stdout", "stderr": label + ".stderr"})
        dump(output / "commands.json", commands)
        with (output / (label + ".stdout")).open("wb") as stdout, (output / (label + ".stderr")).open("wb") as stderr:
            subprocess.run(list(map(str, argv)), cwd=cwd, env=env, stdout=stdout, stderr=stderr, check=True)
        diagnostics = (output / (label + ".stderr")).read_text()
        if (label == "merge" or label.startswith("reports/")) and diagnostics.strip():
            raise ValueError("LLVM emitted diagnostics; inspect " + label + ".stderr")
        result = (output / (label + ".stdout")).read_text()
        if label == "tests" and "LLVM Profile Error" in result + diagnostics:
            raise ValueError("LLVM profile runtime reported an error")
        return result

    before = None
    summary = {"revision": revision, "toolchain": args.toolchain, "status": "failed", "environment_overrides": overrides}
    try:
        run(["git", "archive", "--format=tar", "--output=" + str(output / "source.tar"), revision], "archive", checkout)
        with tarfile.open(output / "source.tar") as archive:
            archive.extractall(source, filter="data")
        before = inventory(source)
        dump(output / "source-before.json", before)
        rustc = run(["rustup", "run", args.toolchain, "rustc", "-vV"], "rustc-version")
        host = re.search(r"^host: (.+)$", rustc, re.M)[1]
        major = re.search(r"LLVM version: (\d+)", rustc)[1]
        sysroot = run(["rustup", "run", args.toolchain, "rustc", "--print", "sysroot"], "sysroot").strip()
        tools = Path(sysroot) / "lib" / "rustlib" / host / "bin"
        suffix = ".exe" if os.name == "nt" else ""
        cov, profdata = [tools / (name + suffix) for name in ("llvm-cov", "llvm-profdata")]
        versions = {"rustc": rustc, "cargo": run(["cargo", "--version"], "cargo-version")}
        for tool in (cov, profdata):
            version = run([tool, "--version"], tool.stem + "-version")
            match = re.search(r"LLVM version (\d+)", version, re.I)
            if not match or match[1] != major:
                raise ValueError("LLVM major mismatch: " + str(tool))
            versions[tool.name] = version
        summary["versions"] = versions
        metadata = json.loads(run(["cargo", "metadata", "--no-deps", "--format-version=1", "--locked", "--offline"], "metadata"))
        ids = set(metadata["workspace_members"])
        package_dirs = [Path(p["manifest_path"]).parent.relative_to(source) for p in metadata["packages"] if p["id"] in ids]
        # Native consumers inherit the same toolchain, flags and absolute profile pattern.
        stdout = run(["cargo", "test", "--workspace", "--locked", "--offline", "--message-format=json"], "tests")
        summary["tests"] = test_counts(stdout)
        records = []
        for line in stdout.splitlines():
            try:
                record = json.loads(line)
            except ValueError:
                continue
            if isinstance(record, dict):
                records.append(record)
        objects = select_objects(records, ids)
        if not objects or any(not Path(p).is_file() for p in objects):
            raise ValueError("missing workspace coverage objects")
        profiles = sorted((output / "profiles").glob("*.profraw"))
        if not profiles:
            raise ValueError("no raw profiles")
        response = output / "profiles.txt"
        if any("\n" in str(p) for p in profiles):
            raise ValueError("newline in profile path")
        response.write_text("".join(str(p) + "\n" for p in profiles))
        merged = output / "merged.profdata"
        run([profdata, "merge", "-sparse", "-f", response, "-o", merged], "merge")
        common = [objects[0], "-instr-profile=" + str(merged)]
        for obj in objects[1:]:
            common.extend(["-object", obj])
        exported = json.loads(run([cov, "export", "--format=text", *common], "reports/full-export"))
        workspace, library, classification = classify_export(exported, source, package_dirs)
        summary.update(workspace=aggregate(workspace), library=aggregate(library), objects=len(objects), profiles=len(profiles))
        for name, files in (("workspace", workspace), ("library", library)):
            run([cov, "report", *common, *sorted({f["filename"] for f in files})], "reports/" + name)
        rows = ["covered\tcount\tpercent\tfile"]
        for item in sorted(library, key=lambda f: f["filename"]):
            totals = aggregate([item])
            rows.append(f'{totals["covered"]}\t{totals["count"]}\t{totals["percent"]}\t{Path(item["filename"]).relative_to(source).as_posix()}')
        (output / "reports" / "perfile-library.txt").write_text("\n".join(rows) + "\n")
        dump(output / "fileclassification.json", classification)
        dump(output / "coverage-inputs.json", {"objects": [str(Path(p).relative_to(output)) for p in objects], "profiles": [str(p.relative_to(output)) for p in profiles]})
        summary["caveats"] = ["Mapped line coverage only; no branch coverage.", "Library scope is file-based; inline cfg(test) helpers may remain, not pure production coverage.", "All profiles are merged, including native consumers; mappings are limited to selected workspace executables and proc-macro libraries.", "Temporary native objects deleted by test harnesses are not recovered; this is not full generated-code coverage.", "Dependency cache and Cargo configuration are inherited; conflicting compiler environment overrides are rejected."]
        summary["status"] = "ok"
    finally:
        if before is not None:
            after = inventory(source)
            dump(output / "source-after.json", after)
            summary["source_unchanged"] = before == after
            if before != after:
                summary["status"] = "failed"
        dump(output / "summary.json", summary)
        artifacts = []
        for path in sorted(output.rglob("*")):
            if path.is_file() and not any(part in {"source", "target", "profiles"} for part in path.relative_to(output).parts):
                artifacts.append({"path": path.relative_to(output).as_posix(), "bytes": path.stat().st_size})
        dump(output / "artifactmanifest.json", artifacts)
        if before is not None and not summary.get("source_unchanged"):
            raise RuntimeError("archived input tree changed; see source inventories")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
