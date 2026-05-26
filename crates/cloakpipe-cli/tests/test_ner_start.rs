//! Integration tests for the `cloakpipe ner start` CLI command.

use std::fs;
use std::process::Command;

#[test]
fn test_ner_start_dry_run_uses_local_virtualenv_when_present() {
    let dir = tempfile::tempdir().unwrap();
    let config_home = tempfile::tempdir().unwrap();
    let root = dir.path();
    let nested = root.join("workspace").join("subdir");
    let venv_python = config_home.path().join(venv_python_path());

    fs::create_dir_all(root.join("tools")).unwrap();
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        root.join("tools").join("gliner-pii-server.py"),
        "print('ok')\n",
    )
    .unwrap();
    fs::create_dir_all(venv_python.parent().unwrap()).unwrap();
    fs::write(&venv_python, "").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .current_dir(&nested)
        .env("CLOAKPIPE_HOME", config_home.path())
        .args([
            "ner",
            "start",
            "--dry-run",
            "--host",
            "0.0.0.0",
            "--port",
            "9222",
            "--threshold",
            "0.55",
        ])
        .output()
        .expect("failed to run cloakpipe ner start --dry-run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "ner start dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Starting GLiNER-PII sidecar..."));
    assert!(stdout.contains("gliner-pii-venv"));
    assert!(stdout.contains("tools/gliner-pii-server.py"));
    assert!(stdout.contains("--host 0.0.0.0 --port 9222 --threshold 0.55"));
}

#[test]
fn test_ner_start_dry_run_fails_outside_cloakpipe_checkout() {
    let dir = tempfile::tempdir().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .current_dir(dir.path())
        .args(["ner", "start", "--dry-run"])
        .output()
        .expect("failed to run cloakpipe ner start --dry-run outside repo");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "ner start should fail outside a checkout"
    );
    assert!(stderr.contains("Could not find tools/gliner-pii-server.py"));
}

fn venv_python_path() -> &'static str {
    if cfg!(windows) {
        "gliner-pii-venv/Scripts/python.exe"
    } else {
        "gliner-pii-venv/bin/python"
    }
}
