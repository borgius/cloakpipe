#![cfg(unix)]

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use reqwest::Client;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

struct StartedSidecar {
    child: Child,
    port: u16,
}

impl Drop for StartedSidecar {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[tokio::test]
async fn test_ner_install_and_start_expose_working_sidecar() {
    let fixture = NerFixture::new().unwrap();
    let sidecar = fixture.install_and_start().await;
    let client = Client::new();

    let health = client
        .get(format!("http://127.0.0.1:{}/health", sidecar.port))
        .send()
        .await
        .unwrap();
    assert!(health.status().is_success());
    assert_eq!(
        health.json::<serde_json::Value>().await.unwrap()["status"],
        "ok"
    );

    let detected = client
        .post(format!("http://127.0.0.1:{}/detect", sidecar.port))
        .json(&serde_json::json!({
            "text": NerFixture::sample_text(),
            "threshold": 0.4,
        }))
        .send()
        .await
        .unwrap();

    assert!(detected.status().is_success());
    let body = detected.json::<serde_json::Value>().await.unwrap();
    let entities = body["entities"].as_array().unwrap();
    assert!(entities.iter().any(|entity| entity["label"] == "person"));
    assert!(entities.iter().any(|entity| entity["label"] == "company_name"));
    assert!(entities.iter().any(|entity| entity["label"] == "city"));
    assert!(entities.iter().any(|entity| entity["label"] == "date"));
}

#[tokio::test]
async fn test_cloakpipe_test_command_uses_gliner_sidecar_entities() {
    let fixture = NerFixture::new().unwrap();
    let sidecar = fixture.install_and_start().await;
    let config_path = fixture.write_config(sidecar.port).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
        .current_dir(fixture.project_root())
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "test",
            "--text",
            NerFixture::sample_text(),
        ])
        .output()
        .expect("failed to run cloakpipe test with gliner-pii sidecar");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "cloakpipe test should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Alice Example"));
    assert!(stdout.contains("Acme Corp"));
    assert!(stdout.contains("Seattle"));
    assert!(stdout.contains("March 5 2026"));
    assert!(stdout.contains("source: Ner"));
    assert!(stdout.contains("PERSON_1"));
    assert!(stdout.contains("ORG_1"));
    assert!(stdout.contains("LOC_1"));
    assert!(stdout.contains("DATE_1"));
    assert!(stdout.contains("Roundtrip match: YES"));
}

struct NerFixture {
    tempdir: tempfile::TempDir,
    real_python: String,
}

impl NerFixture {
    fn new() -> anyhow::Result<Self> {
        let tempdir = tempfile::tempdir()?;
        let real_python = detect_real_python()?;
        let fixture = Self {
            tempdir,
            real_python,
        };
        fixture.prepare_project()?;
        Ok(fixture)
    }

    fn project_root(&self) -> &Path {
        self.tempdir.path()
    }

    fn prepare_project(&self) -> anyhow::Result<()> {
        let tools_dir = self.project_root().join("tools");
        let fake_site = self.project_root().join("fake-gliner-site");
        fs::create_dir_all(&tools_dir)?;
        fs::create_dir_all(&fake_site)?;
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/gliner-pii-server.py"),
            tools_dir.join("gliner-pii-server.py"),
        )?;
        fs::write(fake_site.join("gliner.py"), fake_gliner_module())?;
        write_executable(
            &self.project_root().join("fake-python"),
            &fake_python_script(&self.real_python, &fake_site),
        )?;
        Ok(())
    }

    async fn install_and_start(&self) -> StartedSidecar {
        let install_output = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
            .current_dir(self.project_root())
            .args([
                "ner",
                "install",
                "--python",
                self.project_root().join("fake-python").to_str().unwrap(),
                "--backend",
                "gliner-pii",
            ])
            .output()
            .expect("failed to run cloakpipe ner install");

        assert!(
            install_output.status.success(),
            "cloakpipe ner install should succeed: {}",
            String::from_utf8_lossy(&install_output.stderr)
        );
        let install_stdout = String::from_utf8_lossy(&install_output.stdout);
        assert!(install_stdout.contains("Falling back to a local virtualenv"));
        assert!(install_stdout.contains("Installed gliner successfully."));
        assert!(
            self.project_root()
                .join(".cloakpipe/gliner-pii-venv/bin/python")
                .exists()
        );

        let port = free_port();
        let child = Command::new(env!("CARGO_BIN_EXE_cloakpipe"))
            .current_dir(self.project_root())
            .args([
                "ner",
                "start",
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--threshold",
                "0.4",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to run cloakpipe ner start");

        let sidecar = StartedSidecar { child, port };
        wait_for_sidecar(port).await;
        sidecar
    }

    fn write_config(&self, port: u16) -> anyhow::Result<PathBuf> {
        let config_path = self.project_root().join("cloakpipe.toml");
        fs::write(
            &config_path,
            format!(
                r#"[proxy]
listen = "127.0.0.1:0"
upstream = "https://api.openai.com"
api_key_env = "OPENAI_API_KEY"
timeout_seconds = 120
max_concurrent = 256
mode = "proxy"
masking_strategy = "token"

[vault]
path = "{vault_path}"
encryption = "aes-256-gcm"
key_env = "CLOAKPIPE_VAULT_KEY"
key_keyring = false
backend = "file"

[detection]
secrets = false
financial = false
dates = false
emails = false
phone_numbers = false
ip_addresses = false
urls_internal = false

[detection.ner]
enabled = true
backend = "gliner_pii"
confidence_threshold = 0.4
sidecar_url = "http://127.0.0.1:{port}"
"#,
                vault_path = self.project_root().join("vault.enc").display(),
            ),
        )?;
        Ok(config_path)
    }

    fn sample_text() -> &'static str {
        "Alice Example met Acme Corp in Seattle on March 5 2026."
    }
}

fn detect_real_python() -> anyhow::Result<String> {
    for candidate in ["python3", "python"] {
        if Command::new(candidate)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
        {
            return Ok(candidate.to_string());
        }
    }

    anyhow::bail!("python3 or python is required for GLiNER integration tests")
}

fn wait_for_sidecar(port: u16) -> impl std::future::Future<Output = ()> {
    async move {
        let client = Client::new();
        for _ in 0..50 {
            if let Ok(response) = client
                .get(format!("http://127.0.0.1:{port}/health"))
                .send()
                .await
            {
                if response.status().is_success() {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("GLiNER sidecar did not become healthy on port {port}");
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("failed to allocate free TCP port")
        .local_addr()
        .unwrap()
        .port()
}

fn write_executable(path: &Path, contents: &str) -> anyhow::Result<()> {
    fs::write(path, contents)?;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

fn fake_python_script(real_python: &str, fake_site: &Path) -> String {
    format!(
        r#"#!/bin/sh
set -eu

REAL_PYTHON="{real_python}"
FAKE_SITE="{fake_site}"

if [ "$#" -ge 4 ] && [ "$1" = "-m" ] && [ "$2" = "pip" ] && [ "$3" = "install" ] && [ "$4" = "gliner" ]; then
    case "$0" in
        *gliner-pii-venv*)
            exit 0
            ;;
        *)
            echo "error: externally-managed-environment" >&2
            exit 1
            ;;
    esac
fi

if [ "$#" -ge 3 ] && [ "$1" = "-m" ] && [ "$2" = "venv" ]; then
    venv_dir="$3"
    mkdir -p "$venv_dir/bin"
    cat > "$venv_dir/bin/python" <<'EOF'
#!/bin/sh
set -eu
REAL_PYTHON="{real_python}"
FAKE_SITE="{fake_site}"

if [ "$#" -ge 4 ] && [ "$1" = "-m" ] && [ "$2" = "pip" ] && [ "$3" = "install" ] && [ "$4" = "gliner" ]; then
    exit 0
fi

exec env PYTHONPATH="$FAKE_SITE${{PYTHONPATH:+:$PYTHONPATH}}" "$REAL_PYTHON" "$@"
EOF
    chmod +x "$venv_dir/bin/python"
    exit 0
fi

exec env PYTHONPATH="$FAKE_SITE${{PYTHONPATH:+:$PYTHONPATH}}" "$REAL_PYTHON" "$@"
"#,
        fake_site = fake_site.display(),
    )
}

fn fake_gliner_module() -> &'static str {
    r#"class GLiNER:
    @classmethod
    def from_pretrained(cls, _name):
        return cls()

    def predict_entities(self, text, _labels, threshold=0.4):
        entities = []
        for value, label, score in [
            ("Alice", "first_name", 0.99),
            ("Example", "last_name", 0.98),
            ("Acme Corp", "company_name", 0.97),
            ("Seattle", "city", 0.96),
            ("March 5 2026", "date", 0.95),
        ]:
            start = text.find(value)
            if start >= 0:
                entities.append(
                    {
                        "text": value,
                        "label": label,
                        "start": start,
                        "end": start + len(value),
                        "score": score,
                    }
                )
        return [entity for entity in entities if entity["score"] >= threshold]
"#
}
