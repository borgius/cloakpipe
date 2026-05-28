//! TLS certificate helpers for opt-in HTTPS inspection.
//!
//! The proxy never generates or trusts this CA implicitly on startup. Users must
//! create it with the CLI and trust it in their OS/runtime trust store before
//! enabling `proxy.http_proxy.inspect_https = true`.

use anyhow::{bail, Context, Result};
use cloakpipe_core::{config::HttpProxyConfig, paths};
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose,
};
use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer},
    ServerConfig,
};
use std::{
    fs,
    io::BufReader,
    path::{Path, PathBuf},
    sync::Arc,
};

pub const CA_COMMON_NAME: &str = "CloakPipe Local MITM CA";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MitmPaths {
    pub ca_cert_path: PathBuf,
    pub ca_key_path: PathBuf,
    pub cert_cache_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaStatus {
    pub paths: MitmPaths,
    pub ca_cert_exists: bool,
    pub ca_key_exists: bool,
    pub cert_cache_dir_exists: bool,
}

pub fn resolve_paths(config: &HttpProxyConfig) -> Result<MitmPaths> {
    Ok(MitmPaths {
        ca_cert_path: match &config.ca_cert_path {
            Some(path) => PathBuf::from(path),
            None => paths::global_ca_cert_path()?,
        },
        ca_key_path: match &config.ca_key_path {
            Some(path) => PathBuf::from(path),
            None => paths::global_ca_key_path()?,
        },
        cert_cache_dir: match &config.cert_cache_dir {
            Some(path) => PathBuf::from(path),
            None => paths::global_cert_cache_dir()?,
        },
    })
}

pub fn ca_status(config: &HttpProxyConfig) -> Result<CaStatus> {
    let paths = resolve_paths(config)?;
    Ok(CaStatus {
        ca_cert_exists: paths.ca_cert_path.is_file(),
        ca_key_exists: paths.ca_key_path.is_file(),
        cert_cache_dir_exists: paths.cert_cache_dir.is_dir(),
        paths,
    })
}

pub fn ensure_root_ca(config: &HttpProxyConfig, force: bool) -> Result<CaStatus> {
    let paths = resolve_paths(config)?;
    if (paths.ca_cert_path.exists() || paths.ca_key_path.exists()) && !force {
        ensure_ca_files_present(&paths)?;
        fs::create_dir_all(&paths.cert_cache_dir).with_context(|| {
            format!(
                "Cannot create certificate cache directory: {}",
                paths.cert_cache_dir.display()
            )
        })?;
        return ca_status(config);
    }

    if let Some(parent) = paths.ca_cert_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Cannot create CA certificate directory: {}",
                parent.display()
            )
        })?;
    }
    if let Some(parent) = paths.ca_key_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create CA key directory: {}", parent.display()))?;
    }
    if force && paths.cert_cache_dir.exists() {
        fs::remove_dir_all(&paths.cert_cache_dir).with_context(|| {
            format!(
                "Cannot clear stale certificate cache directory: {}",
                paths.cert_cache_dir.display()
            )
        })?;
    }
    fs::create_dir_all(&paths.cert_cache_dir).with_context(|| {
        format!(
            "Cannot create certificate cache directory: {}",
            paths.cert_cache_dir.display()
        )
    })?;

    let key_pair = KeyPair::generate().context("Failed to generate CloakPipe root CA key")?;
    let mut params = CertificateParams::default();
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, CA_COMMON_NAME);
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    params.use_authority_key_identifier_extension = true;

    let cert = params
        .self_signed(&key_pair)
        .context("Failed to generate CloakPipe root CA certificate")?;

    write_pem_file(&paths.ca_cert_path, &cert.pem(), false)?;
    write_pem_file(&paths.ca_key_path, &key_pair.serialize_pem(), true)?;
    ca_status(config)
}

pub fn require_ca_ready(config: &HttpProxyConfig) -> Result<MitmPaths> {
    let paths = resolve_paths(config)?;
    ensure_ca_files_present(&paths)?;
    Ok(paths)
}

pub fn server_config_for_host(config: &HttpProxyConfig, host: &str) -> Result<Arc<ServerConfig>> {
    let paths = require_ca_ready(config)?;
    let host_cert = host_cert_paths(&paths.cert_cache_dir, host);

    if !host_cert.cert_path.is_file() || !host_cert.key_path.is_file() {
        generate_host_certificate(&paths, &host_cert, host)?;
    }

    let mut cert_chain = load_cert_chain(&host_cert.cert_path)?;
    cert_chain.extend(load_cert_chain(&paths.ca_cert_path)?);
    let key = load_private_key(&host_cert.key_path)?;

    let server_config =
        ServerConfig::builder_with_provider(rustls::crypto::ring::default_provider().into())
            .with_safe_default_protocol_versions()
            .context("Failed to configure TLS protocol versions")?
            .with_no_client_auth()
            .with_single_cert(cert_chain, key)
            .with_context(|| format!("Failed to build TLS config for {host}"))?;

    Ok(Arc::new(server_config))
}

fn ensure_ca_files_present(paths: &MitmPaths) -> Result<()> {
    match (paths.ca_cert_path.is_file(), paths.ca_key_path.is_file()) {
        (true, true) => Ok(()),
        (false, false) => bail!(
            "CloakPipe MITM CA files are missing. Run `cloakpipe http-proxy ca init` first.\n  cert: {}\n  key:  {}",
            paths.ca_cert_path.display(),
            paths.ca_key_path.display()
        ),
        (false, true) => bail!(
            "CloakPipe MITM CA certificate is missing: {}",
            paths.ca_cert_path.display()
        ),
        (true, false) => bail!(
            "CloakPipe MITM CA private key is missing: {}",
            paths.ca_key_path.display()
        ),
    }
}

#[derive(Debug, Clone)]
struct HostCertPaths {
    cert_path: PathBuf,
    key_path: PathBuf,
}

fn host_cert_paths(cache_dir: &Path, host: &str) -> HostCertPaths {
    let safe_host = safe_cache_name(host);
    HostCertPaths {
        cert_path: cache_dir.join(format!("{safe_host}.crt")),
        key_path: cache_dir.join(format!("{safe_host}.key")),
    }
}

fn generate_host_certificate(
    paths: &MitmPaths,
    host_paths: &HostCertPaths,
    host: &str,
) -> Result<()> {
    fs::create_dir_all(&paths.cert_cache_dir).with_context(|| {
        format!(
            "Cannot create certificate cache directory: {}",
            paths.cert_cache_dir.display()
        )
    })?;

    let ca_cert_pem = fs::read_to_string(&paths.ca_cert_path).with_context(|| {
        format!(
            "Cannot read CA certificate: {}",
            paths.ca_cert_path.display()
        )
    })?;
    let ca_key_pem = fs::read_to_string(&paths.ca_key_path).with_context(|| {
        format!(
            "Cannot read CA private key: {}",
            paths.ca_key_path.display()
        )
    })?;
    let ca_key = KeyPair::from_pem(&ca_key_pem).context("Failed to parse CloakPipe CA key")?;
    let issuer = Issuer::from_ca_cert_pem(&ca_cert_pem, ca_key)
        .context("Failed to parse CloakPipe CA certificate")?;

    let leaf_key = KeyPair::generate().context("Failed to generate host certificate key")?;
    let mut params = CertificateParams::new(vec![host.to_string()])
        .with_context(|| format!("Invalid host for certificate SAN: {host}"))?;
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, host);
    params.is_ca = IsCa::NoCa;
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.use_authority_key_identifier_extension = true;

    let cert = params
        .signed_by(&leaf_key, &issuer)
        .with_context(|| format!("Failed to sign host certificate for {host}"))?;

    write_pem_file(&host_paths.cert_path, &cert.pem(), false)?;
    write_pem_file(&host_paths.key_path, &leaf_key.serialize_pem(), true)?;
    Ok(())
}

fn load_cert_chain(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
    let file = fs::File::open(path)
        .with_context(|| format!("Cannot open certificate file: {}", path.display()))?;
    let mut reader = BufReader::new(file);
    rustls_pemfile::certs(&mut reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("Cannot parse certificate file: {}", path.display()))
}

fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>> {
    let file = fs::File::open(path)
        .with_context(|| format!("Cannot open private key file: {}", path.display()))?;
    let mut reader = BufReader::new(file);
    rustls_pemfile::private_key(&mut reader)
        .with_context(|| format!("Cannot parse private key file: {}", path.display()))?
        .ok_or_else(|| anyhow::anyhow!("No private key found in {}", path.display()))
}

fn write_pem_file(path: &Path, contents: &str, secret: bool) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create directory: {}", parent.display()))?;
    }

    fs::write(path, contents).with_context(|| format!("Cannot write: {}", path.display()))?;

    #[cfg(unix)]
    if secret {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(path, permissions)?;
    }

    Ok(())
}

fn safe_cache_name(host: &str) -> String {
    host.trim_matches(['[', ']'])
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-') {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(temp_dir: &Path) -> HttpProxyConfig {
        HttpProxyConfig {
            ca_cert_path: Some(temp_dir.join("ca.crt").to_string_lossy().into_owned()),
            ca_key_path: Some(temp_dir.join("ca.key").to_string_lossy().into_owned()),
            cert_cache_dir: Some(temp_dir.join("cache").to_string_lossy().into_owned()),
            ..HttpProxyConfig::default()
        }
    }

    #[test]
    fn ensure_root_ca_creates_cert_key_and_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = test_config(temp_dir.path());

        let status = ensure_root_ca(&config, false).unwrap();

        assert!(status.ca_cert_exists);
        assert!(status.ca_key_exists);
        assert!(status.cert_cache_dir_exists);
        assert!(status.paths.ca_cert_path.is_file());
        assert!(status.paths.ca_key_path.is_file());
    }

    #[test]
    fn require_ca_ready_rejects_missing_ca_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = test_config(temp_dir.path());

        let error = require_ca_ready(&config).unwrap_err();

        assert!(error.to_string().contains("ca init"));
    }

    #[test]
    fn server_config_for_host_generates_cached_leaf_certificate() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = test_config(temp_dir.path());
        ensure_root_ca(&config, false).unwrap();

        let server_config = server_config_for_host(&config, "api.openai.test").unwrap();

        assert!(Arc::strong_count(&server_config) >= 1);
        assert!(temp_dir.path().join("cache/api.openai.test.crt").is_file());
        assert!(temp_dir.path().join("cache/api.openai.test.key").is_file());
    }

    #[test]
    fn safe_cache_name_replaces_unsafe_characters() {
        assert_eq!(safe_cache_name("[::1]"), "__1");
        assert_eq!(safe_cache_name("API.OpenAI.Com"), "api.openai.com");
    }
}
