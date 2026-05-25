//! Integration tests for policy editing command wiring.

use std::process::Command;

#[test]
fn policy_edit_help_is_available() {
    let output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .args(["policy", "edit", "--help"])
        .output()
        .expect("failed to run cloakpipe policy edit --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "policy edit help should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Create or edit the active policy file interactively"));
}
