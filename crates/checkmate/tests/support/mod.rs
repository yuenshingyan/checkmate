//! Shared native-consumer harness for task-013 public workflow regressions.
//! Intentional fixture/baseline edits happen before a run, never during compilation.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub struct Consumer(tempfile::TempDir);

impl Consumer {
    pub fn new() -> Self {
        let this = Self(tempfile::tempdir().unwrap());
        this.write("Cargo.toml", &format!("[package]\nname = \"regression-consumer\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n[dev-dependencies]\ncheckmate = {{ path = {:?} }}\n", env!("CARGO_MANIFEST_DIR")));
        this
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.0.path().join(name)
    }

    pub fn write(&self, name: &str, text: &str) {
        let path = self.path(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    pub fn test(&self, errors: &[&str]) -> String {
        self.run(&["test", "--test", "probe"], errors)
    }

    pub fn run(&self, args: &[&str], errors: &[&str]) -> String {
        let before = inventory(self.0.path());
        let output = Command::new(env!("CARGO"))
            .args(["--offline", "--quiet"])
            .args(args)
            .env("CARGO_TARGET_DIR", self.path("target"))
            .env("CARGO_TERM_COLOR", "never")
            .current_dir(self.0.path())
            .output()
            .unwrap();
        let after = inventory(self.0.path());
        for (path, bytes) in &before {
            assert_eq!(
                after.get(path),
                Some(bytes),
                "Cargo changed or deleted {}",
                path.display()
            );
        }
        for path in after.keys().filter(|path| !before.contains_key(*path)) {
            // Cargo may create its own lockfile; no other new input/report is allowed.
            assert_eq!(
                path,
                Path::new("Cargo.lock"),
                "Cargo created unexpected input {}",
                path.display()
            );
        }
        let report = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(
            output.status.success(),
            errors.is_empty(),
            "{args:?}:\n{report}"
        );
        for forbidden in [
            "proc macro panicked",
            "automatic analysis failed",
            "procedural macro API is used outside",
        ] {
            assert!(!report.contains(forbidden), "{forbidden}:\n{report}");
        }
        for expected in errors {
            assert!(report.contains(expected), "missing {expected:?}:\n{report}");
        }
        report
    }
}

fn inventory(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path == root.join("target") {
                continue;
            }
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().into(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}
