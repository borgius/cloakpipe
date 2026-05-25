//! Integration tests for bundled preset installation and resolution.

use std::fs;
use std::process::Command;

#[test]
fn presets_install_writes_bundled_files_to_config_home() {
    let config_home = tempfile::tempdir().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .env("CLOAKPIPE_CONFIG_HOME", config_home.path())
        .args(["presets", "install"])
        .output()
        .expect("failed to run cloakpipe presets install");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "presets install should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Bundled presets installed in"));

    let preset_dir = config_home.path().join("policies");
    assert!(
        preset_dir.join("dpdp.toml").exists(),
        "dpdp preset should be installed"
    );
    assert!(
        preset_dir.join("pci-dss.toml").exists(),
        "pci-dss preset should be installed"
    );

    let dpdp = fs::read_to_string(preset_dir.join("dpdp.toml")).unwrap();
    assert!(dpdp.contains("DPDP Act 2023"));
}

#[test]
fn test_command_uses_embedded_preset_name_and_installs_presets() {
    let config_home = tempfile::tempdir().unwrap();
    let working_dir = tempfile::tempdir().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .current_dir(working_dir.path())
        .env("CLOAKPIPE_CONFIG_HOME", config_home.path())
        .args([
            "--config",
            "dpdp.toml",
            "test",
            "--text",
            "Contact rajesh@okicici with GSTIN 27AAPFU0939F1ZV",
        ])
        .output()
        .expect("failed to run cloakpipe --config dpdp.toml test");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "preset-based test command should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("UPI_ID"),
        "stdout should show the UPI_ID category: {stdout}"
    );
    assert!(
        stdout.contains("GSTIN"),
        "stdout should show the GSTIN category: {stdout}"
    );

    let preset_dir = config_home.path().join("policies");
    assert!(
        preset_dir.join("dpdp.toml").exists(),
        "dpdp preset should be installed on first use"
    );
    assert!(
        preset_dir.join("gdpr.toml").exists(),
        "all bundled presets should be installed on first use"
    );
}
