//! Shared path helpers for CloakPipe home, config discovery, and model storage.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// Canonical global CloakPipe home override.
pub const CLOAKPIPE_HOME_ENV: &str = "CLOAKPIPE_HOME";
/// Backward-compatible config-home override used by earlier releases.
pub const CLOAKPIPE_CONFIG_HOME_ENV: &str = "CLOAKPIPE_CONFIG_HOME";

/// Canonical project/global config filename.
pub const CANONICAL_CONFIG_FILE: &str = "cloakpipe.toml";
/// Compatibility alias for the requested misspelling.
pub const REQUESTED_ALIAS_CONFIG_FILE: &str = "cloackpipe.toml";

/// Canonical global home directory name.
pub const CANONICAL_HOME_DIR: &str = ".cloakpipe";
/// Compatibility alias for the requested misspelling.
pub const REQUESTED_ALIAS_HOME_DIR: &str = ".cloackpipe";

pub const MODELS_DIR: &str = "models";
pub const POLICIES_DIR: &str = "policies";
pub const CERTS_DIR: &str = "certs";
pub const CERT_CACHE_DIR: &str = "cache";
pub const CA_CERT_FILE: &str = "cloakpipe-ca.crt";
pub const CA_KEY_FILE: &str = "cloakpipe-ca.key";

/// Config filenames to consider while walking project ancestors.
pub const fn project_config_file_names() -> [&'static str; 2] {
    [CANONICAL_CONFIG_FILE, REQUESTED_ALIAS_CONFIG_FILE]
}

/// Resolve the global CloakPipe home.
///
/// Precedence:
/// 1. `CLOAKPIPE_HOME`
/// 2. legacy `CLOAKPIPE_CONFIG_HOME`
/// 3. `~/.cloakpipe`
/// 4. existing `~/.cloackpipe` compatibility alias when canonical does not exist
pub fn global_home() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os(CLOAKPIPE_HOME_ENV) {
        return Ok(PathBuf::from(dir));
    }

    if let Some(dir) = std::env::var_os(CLOAKPIPE_CONFIG_HOME_ENV) {
        return Ok(PathBuf::from(dir));
    }

    let Some(home) = std::env::var_os("HOME") else {
        bail!(
            "Cannot determine the CloakPipe home directory. Set {} to a writable directory.",
            CLOAKPIPE_HOME_ENV
        );
    };

    let home = PathBuf::from(home);
    let canonical = home.join(CANONICAL_HOME_DIR);
    let alias = home.join(REQUESTED_ALIAS_HOME_DIR);
    if !canonical.exists() && alias.exists() {
        return Ok(alias);
    }

    Ok(canonical)
}

pub fn global_config_path() -> Result<PathBuf> {
    Ok(global_home()?.join(CANONICAL_CONFIG_FILE))
}

pub fn global_alias_config_path() -> Result<PathBuf> {
    Ok(global_home()?.join(REQUESTED_ALIAS_CONFIG_FILE))
}

pub fn global_config_candidates() -> Result<Vec<PathBuf>> {
    Ok(vec![global_config_path()?, global_alias_config_path()?])
}

pub fn global_models_dir() -> Result<PathBuf> {
    Ok(global_home()?.join(MODELS_DIR))
}

pub fn global_policies_dir() -> Result<PathBuf> {
    Ok(global_home()?.join(POLICIES_DIR))
}

pub fn global_certs_dir() -> Result<PathBuf> {
    Ok(global_home()?.join(CERTS_DIR))
}

pub fn global_ca_cert_path() -> Result<PathBuf> {
    Ok(global_certs_dir()?.join(CA_CERT_FILE))
}

pub fn global_ca_key_path() -> Result<PathBuf> {
    Ok(global_certs_dir()?.join(CA_KEY_FILE))
}

pub fn global_cert_cache_dir() -> Result<PathBuf> {
    Ok(global_certs_dir()?.join(CERT_CACHE_DIR))
}

pub fn global_distilbert_pii_dir() -> Result<PathBuf> {
    Ok(global_models_dir()?.join("distilbert-pii"))
}

pub fn global_distilbert_pii_model_path() -> Result<PathBuf> {
    Ok(global_distilbert_pii_dir()?
        .join("quantized")
        .join("model_quantized.onnx"))
}

pub fn global_bert_ner_model_path() -> Result<PathBuf> {
    Ok(global_models_dir()?.join("bert-ner.onnx"))
}

pub fn global_gliner_model_path() -> Result<PathBuf> {
    Ok(global_models_dir()?.join("gliner.onnx"))
}

pub fn global_gliner_runtime_dir() -> Result<PathBuf> {
    Ok(global_home()?.join("gliner-pii-venv"))
}

/// Resolve a config-relative path without canonicalizing or requiring it to exist.
pub fn resolve_config_relative_path(base_dir: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

pub fn resolve_config_relative_string(base_dir: &Path, value: &str) -> String {
    resolve_config_relative_path(base_dir, value)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_config_names_include_canonical_then_alias() {
        assert_eq!(
            project_config_file_names(),
            [CANONICAL_CONFIG_FILE, REQUESTED_ALIAS_CONFIG_FILE]
        );
    }

    #[test]
    fn config_relative_paths_keep_absolute_paths() {
        let absolute = if cfg!(windows) {
            "C:\\vault.db"
        } else {
            "/tmp/vault.db"
        };
        assert_eq!(
            resolve_config_relative_path(Path::new("/base"), absolute),
            PathBuf::from(absolute)
        );
    }

    #[test]
    fn config_relative_paths_join_relative_paths() {
        assert_eq!(
            resolve_config_relative_path(Path::new("/base/project"), "vault.db"),
            PathBuf::from("/base/project/vault.db")
        );
    }

    #[test]
    fn global_cert_paths_live_under_certs_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let _guard = EnvVarGuard::set("CLOAKPIPE_HOME", temp_dir.path());

        assert_eq!(global_certs_dir().unwrap(), temp_dir.path().join("certs"));
        assert_eq!(
            global_ca_cert_path().unwrap(),
            temp_dir.path().join("certs/cloakpipe-ca.crt")
        );
        assert_eq!(
            global_ca_key_path().unwrap(),
            temp_dir.path().join("certs/cloakpipe-ca.key")
        );
        assert_eq!(
            global_cert_cache_dir().unwrap(),
            temp_dir.path().join("certs/cache")
        );
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &Path) -> Self {
            let previous = std::env::var_os(key);
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }
}
