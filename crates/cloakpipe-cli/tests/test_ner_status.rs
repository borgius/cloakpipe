//! Integration tests for the `cloakpipe ner status` CLI command.

use std::fs;
use std::process::Command;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[test]
fn test_ner_status_json_reports_missing_distilbert_and_download_hint() {
    let config_home = tempfile::tempdir().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .env("CLOAKPIPE_HOME", config_home.path())
        .args(["ner", "status", "--json"])
        .output()
        .expect("failed to run cloakpipe ner status --json");

    let body: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("ner status --json should emit valid JSON");

    assert!(
        output.status.success(),
        "ner status --json should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let models = body["models"].as_array().unwrap();
    let backends: Vec<&str> = models
        .iter()
        .map(|model| model["backend"].as_str().unwrap())
        .collect();

    assert_eq!(backends, vec!["distilbert_pii", "gliner_pii", "bert", "gliner"]);
    assert_eq!(body["models"][0]["backend"], "distilbert_pii");
    assert_eq!(
        body["models"][0]["name"],
        "DistilBERT-PII NER (63MB ONNX, 33 entity types, runs on any CPU)"
    );
    assert_eq!(body["models"][0]["status"], "missing");
    assert_eq!(body["models"][0]["download_command"], "cloakpipe ner download");
    assert_eq!(body["models"][1]["backend"], "gliner_pii");
    assert_eq!(
        body["models"][1]["download_command"],
        "cloakpipe ner download --model gliner_pii"
    );
}

#[test]
fn test_ner_status_reports_missing_distilbert_and_download_hint() {
    let config_home = tempfile::tempdir().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .env("CLOAKPIPE_HOME", config_home.path())
        .args(["ner", "status"])
        .output()
        .expect("failed to run cloakpipe ner status");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "ner status should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("NER model status"), "stdout was: {stdout}");
    assert!(
        stdout.contains("DistilBERT-PII NER (63MB ONNX, 33 entity types, runs on any CPU)"),
        "stdout was: {stdout}"
    );
    assert!(
        stdout.contains("GLiNER-PII sidecar (managed Python runtime for custom entity types)"),
        "stdout was: {stdout}"
    );
    assert!(stdout.contains("BERT NER (legacy 4-type ONNX model)"), "stdout was: {stdout}");
    assert!(stdout.contains("GLiNER2 (legacy zero-shot ONNX model)"), "stdout was: {stdout}");
    assert!(stdout.contains("status: missing"), "stdout was: {stdout}");
    assert!(
        stdout.contains("download: cloakpipe ner download"),
        "stdout was: {stdout}"
    );
}

#[test]
fn test_ner_status_reports_incomplete_distilbert_download() {
    let config_home = tempfile::tempdir().unwrap();
    let distilbert_dir = config_home.path().join("models/distilbert-pii");

    fs::create_dir_all(&distilbert_dir).unwrap();
    fs::write(distilbert_dir.join("tokenizer.json"), "{}\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .env("CLOAKPIPE_HOME", config_home.path())
        .args(["ner", "status"])
        .output()
        .expect("failed to run cloakpipe ner status with partial DistilBERT files");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "ner status should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("status: incomplete"), "stdout was: {stdout}");
    assert!(
        stdout.contains("download: cloakpipe ner download"),
        "stdout was: {stdout}"
    );
}

#[test]
fn test_ner_status_json_lists_all_available_models() {
    let config_home = tempfile::tempdir().unwrap();
    let models_dir = config_home.path().join("models");
    let distilbert_dir = models_dir.join("distilbert-pii");
    let gliner_runtime_python = config_home.path().join(venv_python_path());

    fs::create_dir_all(distilbert_dir.join("quantized")).unwrap();
    fs::write(
        distilbert_dir.join("quantized/model_quantized.onnx"),
        "onnx\n",
    )
    .unwrap();
    fs::write(distilbert_dir.join("config.json"), "{}\n").unwrap();
    fs::write(distilbert_dir.join("tokenizer_config.json"), "{}\n").unwrap();
    fs::write(distilbert_dir.join("special_tokens_map.json"), "{}\n").unwrap();
    fs::write(distilbert_dir.join("tokenizer.json"), "{}\n").unwrap();
    fs::create_dir_all(gliner_runtime_python.parent().unwrap()).unwrap();
    fs::write(&gliner_runtime_python, "#!/bin/sh\nexit 0\n").unwrap();
    make_executable_if_needed(&gliner_runtime_python);
    fs::write(models_dir.join("bert-ner.onnx"), "onnx\n").unwrap();
    fs::write(models_dir.join("gliner.onnx"), "onnx\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .env("CLOAKPIPE_HOME", config_home.path())
        .args(["ner", "status", "--json"])
        .output()
        .expect("failed to run cloakpipe ner status --json with downloaded models");

    let body: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("ner status --json should emit valid JSON");
    let models = body["models"].as_array().unwrap();
    let backends: Vec<&str> = models
        .iter()
        .map(|model| model["backend"].as_str().unwrap())
        .collect();

    assert!(
        output.status.success(),
        "ner status --json should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(backends, vec!["distilbert_pii", "gliner_pii", "bert", "gliner"]);
    assert_eq!(models[0]["status"], "installed");
    assert_eq!(models[1]["status"], "installed");
    assert_eq!(models[0]["download_command"], serde_json::Value::Null);
}

#[test]
fn test_ner_status_lists_all_available_models() {
    let config_home = tempfile::tempdir().unwrap();
    let models_dir = config_home.path().join("models");
    let distilbert_dir = models_dir.join("distilbert-pii");
    let gliner_runtime_python = config_home.path().join(venv_python_path());

    fs::create_dir_all(distilbert_dir.join("quantized")).unwrap();
    fs::write(
        distilbert_dir.join("quantized/model_quantized.onnx"),
        "onnx\n",
    )
    .unwrap();
    fs::write(distilbert_dir.join("config.json"), "{}\n").unwrap();
    fs::write(distilbert_dir.join("tokenizer_config.json"), "{}\n").unwrap();
    fs::write(distilbert_dir.join("special_tokens_map.json"), "{}\n").unwrap();
    fs::write(distilbert_dir.join("tokenizer.json"), "{}\n").unwrap();
    fs::create_dir_all(gliner_runtime_python.parent().unwrap()).unwrap();
    fs::write(&gliner_runtime_python, "#!/bin/sh\nexit 0\n").unwrap();
    make_executable_if_needed(&gliner_runtime_python);
    fs::write(models_dir.join("bert-ner.onnx"), "onnx\n").unwrap();
    fs::write(models_dir.join("gliner.onnx"), "onnx\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .env("CLOAKPIPE_HOME", config_home.path())
        .args(["ner", "status"])
        .output()
        .expect("failed to run cloakpipe ner status with downloaded models");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "ner status should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("DistilBERT-PII NER (63MB ONNX, 33 entity types, runs on any CPU)"),
        "stdout was: {stdout}"
    );
    assert!(
        stdout.contains("GLiNER-PII sidecar (managed Python runtime for custom entity types)"),
        "stdout was: {stdout}"
    );
    assert!(
        stdout.contains("BERT NER (legacy 4-type ONNX model)"),
        "stdout was: {stdout}"
    );
    assert!(
        stdout.contains("GLiNER2 (legacy zero-shot ONNX model)"),
        "stdout was: {stdout}"
    );
    assert!(!stdout.contains("download: cloakpipe ner download"), "stdout was: {stdout}");
}

fn venv_python_path() -> &'static str {
    if cfg!(windows) {
        "gliner-pii-venv/Scripts/python.exe"
    } else {
        "gliner-pii-venv/bin/python"
    }
}

fn make_executable_if_needed(path: &std::path::Path) {
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }
}