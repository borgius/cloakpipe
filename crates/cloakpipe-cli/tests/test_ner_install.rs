//! Integration tests for the `cloakpipe ner install` CLI command.

use std::fs;
use std::process::Command;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[test]
fn test_ner_install_dry_run() {
    let output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .args([
            "ner",
            "install",
            "--dry-run",
            "--python",
            "python3",
            "--backend",
            "gliner-pii",
        ])
        .output()
        .expect("failed to run cloakpipe ner install --dry-run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "ner install dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Would run: python3 -m pip install"), "expected pip install command in: {stdout}");
    assert!(stdout.contains("gliner"), "expected gliner package in: {stdout}");
    assert!(stdout.contains(".cloakpipe/gliner-pii-venv"));
    assert!(stdout.contains("Start the sidecar: cloakpipe ner start"));
    assert!(stdout.contains("backend = \"gliner_pii\""));
}

#[cfg(unix)]
#[test]
fn test_ner_install_falls_back_to_local_virtualenv_when_python_is_externally_managed() {
    let dir = tempfile::tempdir().unwrap();
    let fake_python = dir.path().join("fake-python");

    fs::write(
        &fake_python,
        r##"#!/bin/sh
set -eu

if [ "$1" = "-m" ] && [ "$2" = "pip" ] && [ "$3" = "install" ]; then
    echo "error: externally-managed-environment" >&2
    exit 1
fi

if [ "$1" = "-m" ] && [ "$2" = "venv" ]; then
    venv_dir="$3"
    mkdir -p "$venv_dir/bin"
    cat > "$venv_dir/bin/python" <<'EOF'
#!/bin/sh
set -eu

if [ "$1" = "-m" ] && [ "$2" = "pip" ] && [ "$3" = "install" ]; then
    exit 0
fi

if [ "$1" = "-c" ]; then
    echo "GLiNER import OK"
    exit 0
fi

echo "unexpected venv python args: $@" >&2
exit 1
EOF
    chmod +x "$venv_dir/bin/python"
    exit 0
fi

echo "unexpected base python args: $@" >&2
exit 1
"##,
    )
    .unwrap();

    let mut permissions = fs::metadata(&fake_python).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_python, permissions).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .current_dir(dir.path())
        .args([
            "ner",
            "install",
            "--python",
            fake_python.to_str().unwrap(),
            "--backend",
            "gliner-pii",
        ])
        .output()
        .expect("failed to run cloakpipe ner install with fake python");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "ner install should succeed via virtualenv fallback: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Detected an externally managed Python environment."));
    assert!(stdout.contains("Falling back to a local virtualenv at .cloakpipe/gliner-pii-venv"));
    assert!(stdout.contains("Installed gliner successfully."));
    assert!(stdout.contains("Start the sidecar: cloakpipe ner start"));
    assert!(dir
        .path()
        .join(".cloakpipe/gliner-pii-venv/bin/python")
        .exists());
}
