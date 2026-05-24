//! Integration tests for the `cloakpipe ner install` CLI command.

use std::process::Command;

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
    assert!(stdout.contains("Would run: python3 -m pip install gliner"));
    assert!(stdout.contains("tools/gliner-pii-server.py"));
    assert!(stdout.contains("backend = \"gliner_pii\""));
}
