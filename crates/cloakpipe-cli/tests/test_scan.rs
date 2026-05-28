//! Integration tests for the `cloakpipe scan` CLI command.

use std::fs;
use std::process::Command;

#[test]
fn test_scan_detect_only() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("test.txt");
    fs::write(
        &input,
        "Contact Rajesh at rajesh@example.com or +91 98765 43210",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .current_dir(dir.path())
        .env("CLOAKPIPE_HOME", dir.path().join("global"))
        .args(["scan", input.to_str().unwrap(), "--detect-only"])
        .output()
        .expect("failed to run cloakpipe");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "scan should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Total entities:"), "Should show summary");
}

#[test]
fn test_scan_mask_output() {
    let dir = tempfile::tempdir().unwrap();
    let input_dir = dir.path().join("docs");
    let output_dir = dir.path().join("docs-masked");
    fs::create_dir(&input_dir).unwrap();
    fs::write(
        input_dir.join("file.txt"),
        "Patient Rajesh Singh, email rajesh@hospital.com, Aadhaar 2345 6789 0123",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .current_dir(dir.path())
        .env("CLOAKPIPE_HOME", dir.path().join("global"))
        .args([
            "scan",
            input_dir.to_str().unwrap(),
            "-o",
            output_dir.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run cloakpipe");

    assert!(
        output.status.success(),
        "scan should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify output file exists and doesn't contain original PII
    let masked = fs::read_to_string(output_dir.join("file.txt")).unwrap();
    assert!(
        !masked.contains("rajesh@hospital.com"),
        "Email should be masked"
    );

    // Verify vault mappings file exists
    assert!(
        output_dir.join("vault-mappings.json").exists(),
        "vault mappings should be exported"
    );
}

#[test]
fn test_scan_no_files() {
    let dir = tempfile::tempdir().unwrap();
    let input_dir = dir.path().join("empty");
    fs::create_dir(&input_dir).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .current_dir(dir.path())
        .env("CLOAKPIPE_HOME", dir.path().join("global"))
        .args(["scan", input_dir.to_str().unwrap(), "--detect-only"])
        .output()
        .expect("failed to run cloakpipe");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("No scannable files"));
}

#[test]
fn test_scan_sample_masks_documented_leaks() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = tempfile::tempdir().unwrap();
    let output_dir = dir.path().join("example-masked");
    let policy_path = dir.path().join("sample-policy.toml");
    let policy = fs::read_to_string(workspace.join("policies/minimal.toml"))
        .unwrap()
        .replace(
            "force = []",
            r#"force = [
  "Avery Collins",
  "Dr. Elena Morris",
  "Avery Collins Family HSA",
  "1842 Willow Creek Drive",
  "Apt 5B",
]"#,
        );
    fs::write(&policy_path, policy).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .current_dir(&workspace)
        .args([
            "--config",
            policy_path.to_str().unwrap(),
            "scan",
            "assets/example.md",
            "-o",
            output_dir.to_str().unwrap(),
            "--strategy",
            "similar",
        ])
        .output()
        .expect("failed to run cloakpipe");

    assert!(
        output.status.success(),
        "scan should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let masked = fs::read_to_string(output_dir.join("example.md")).unwrap();
    for original in [
        "TempPass!2026",
        "356938035643809",
        "4455667788990011",
        "7788990011223344",
        "4111111111111111",
        "5555555555554444",
        "Visa",
        "Mastercard",
        "Avery Collins",
        "Dr. Elena Morris",
        "Avery Collins Family HSA",
        "1842 Willow Creek Drive",
        "Apt 5B",
        "212-555-0176",
    ] {
        assert!(
            !masked.contains(original),
            "masked sample leaked {original}"
        );
    }

    for placeholder in [
        "User-",
        "Org-",
        "Location-",
        "DATE_",
        "PCT-",
        "ID_NUMBER-",
        "LICENSE_NUMBER-",
        "IBAN-",
        "ROUTING_NUMBER-",
        "SWIFT_CODE-",
        "ISIN-",
        "https://masked-",
    ] {
        assert!(
            !masked.contains(placeholder),
            "similar masking should not emit placeholder family {placeholder}"
        );
    }

    let restored_path = dir.path().join("example-restored.md");
    let masked_path = output_dir.join("example.md");
    let restore_output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .current_dir(&workspace)
        .args([
            "restore",
            masked_path.to_str().unwrap(),
            "-o",
            restored_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run cloakpipe restore");

    assert!(
        restore_output.status.success(),
        "restore should succeed: {}",
        String::from_utf8_lossy(&restore_output.stderr)
    );

    let original = fs::read_to_string(workspace.join("assets/example.md")).unwrap();
    let restored = fs::read_to_string(restored_path).unwrap();
    assert_eq!(restored, original, "restored sample should match original");
}

#[test]
fn test_scan_can_disable_ner_with_flag() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = tempfile::tempdir().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .current_dir(&workspace)
        .env("CLOAKPIPE_HOME", dir.path().join("global"))
        .args(["scan", "assets/example.md", "--detect-only", "--no-ner"])
        .output()
        .expect("failed to run cloakpipe");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "scan should succeed with --no-ner: {stderr}"
    );
    assert!(stdout.contains("Files scanned:  1"));
    assert!(stdout.contains("NER:            disabled"));
}
