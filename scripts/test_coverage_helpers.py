"""Pure coverage helper tests; no compiler or subprocess invocation."""
import tempfile
from pathlib import Path
import unittest

from measure_coverage import aggregate, classify_export, classify_file, inventory, select_objects, test_counts


class CoverageHelpers(unittest.TestCase):
    def test_workspace_objects_only(self):
        def artifact(package, executable=None, kind=None, filenames=()):
            return {"reason": "compiler-artifact", "package_id": package,
                    "executable": executable, "target": {"kind": kind or []},
                    "filenames": filenames}
        records = [artifact("own", "/test"), artifact("own", "/test"),
                   artifact("dependency", "/external", ["proc-macro"], ["dep.so"]),
                   artifact("own", kind=["proc-macro"], filenames=["a.dylib", "b.dll", "c.so", "d.rlib", "e.rmeta"]),
                   artifact("own", filenames=["ordinary.so", "lib.rlib"]),
                   {"reason": "build-script-executed", "package_id": "own", "executable": "/wrong"}]
        self.assertEqual(select_objects(records, {"own"}), ["/test", "a.dylib", "b.dll", "c.so"])
        self.assertEqual(select_objects(records, set()), [])

    def test_scope_boundaries_and_library_exclusions(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source"
            source.mkdir()
            cases = {"crates/a/src/lib.rs": "library",
                     "crates/a/src/inline.rs": "library",
                     "crates/a/src/tests.rs": "test file",
                     "crates/a/src/foo_tests.rs": "test file",
                     "crates/a/src/diagnostics/integration.rs": "test file",
                     "crates/a/src/walkthroughs.rs": "test file",
                     "crates/a/src/resolve/graph/properties.rs": "generated property tests",
                     "crates/a/src/tests/nested.rs": "tests/examples directory",
                     "crates/a/tests/integration.rs": "tests/examples directory",
                     "crates/a/examples/demo.rs": "tests/examples directory",
                     "crates/a/build.rs": "workspace non-library",
                     "other/src/lib.rs": "outside workspace packages"}
            for name, reason in cases.items():
                path = source / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("// test\n")
                self.assertEqual(classify_file(str(path), source, [Path("crates/a")]), (name, reason))
            example = source / "examples/demo/src/lib.rs"
            example.parent.mkdir(parents=True)
            example.write_text("")
            self.assertEqual(classify_file(str(example), source, [Path("examples/demo")]),
                             ("examples/demo/src/lib.rs", "tests/examples directory"))
            self.assertEqual(classify_file(str(source) + "-other/src/lib.rs", source, [Path(".")]), (None, "outside archive"))
            outside = Path(directory) / "outside.rs"
            outside.write_text("")
            (source / "link.rs").symlink_to(outside)
            self.assertEqual(classify_file(str(source / "link.rs"), source, [Path(".")]), (None, "outside archive"))

    def test_workspace_and_library_totals_use_the_same_explicit_classification(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory)
            files = []
            for name in ["crates/a/src/lib.rs", "crates/a/src/diagnostics/integration.rs",
                         "examples/demo/src/lib.rs", "other.rs"]:
                path = source / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("// fixture\n")
                files.append({"filename": str(path), "summary": {"lines": {"count": 10, "covered": 7}}})
            workspace, library, rows = classify_export({"data": [{"files": files}]}, source,
                                                      [Path("crates/a"), Path("examples/demo")])
            self.assertEqual(aggregate(workspace)["count"], 30)
            self.assertEqual(aggregate(library)["count"], 10)
            self.assertEqual([row["path"] for row in rows if row["library"]], ["crates/a/src/lib.rs"])
            self.assertFalse(rows[-1]["workspace"])

    def test_line_summary_not_segments(self):
        def file(name, count, covered):
            return {"filename": name, "summary": {"lines": {"count": count, "covered": covered}}, "segments": []}
        a, b = file("a", 10, 7), file("b", 5, 5)
        self.assertEqual(aggregate([a, b, a]), {"count": 15, "covered": 12, "percent": 80.0, "files": 2})
        self.assertIsNone(aggregate([file("empty", 0, 0)])["percent"])
        for files in ([], [a, file("a", 11, 7)], [file("bad", 1, 2)], [file("bad", -1, 0)]):
            with self.assertRaises(ValueError):
                aggregate(files)

    def test_test_counts_mixed_output(self):
        text = '{"reason":"compiler-artifact"}\nrunning 3 tests\ntest result: ok. 2 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out\n'
        text += 'test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out\n'
        self.assertEqual(test_counts(text), {"passed": 5, "failed": 0, "ignored": 1, "measured": 0, "filtered_out": 2, "result_summaries": 2})
        for text in ("no test output", "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out"):
            with self.assertRaises(ValueError):
                test_counts(text)

    def test_inventory_detects_bytes_additions_and_deletions(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "input"
            path.write_bytes(b"before")
            before = inventory(root)
            self.assertEqual(before, inventory(root))
            path.write_bytes(b"after!")
            self.assertNotEqual(before, inventory(root))
            path.write_bytes(b"before")
            (root / "new-dir").mkdir()
            self.assertNotEqual(before, inventory(root))
            path.unlink()
            self.assertNotEqual(before, inventory(root))


if __name__ == "__main__":
    unittest.main()
