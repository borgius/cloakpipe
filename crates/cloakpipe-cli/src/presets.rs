use anyhow::{bail, Context, Result};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

const CLOAKPIPE_CONFIG_HOME_ENV: &str = "CLOAKPIPE_CONFIG_HOME";
const PRESET_INSTALL_SUBDIR: &str = "policies";

pub struct BundledPreset {
    pub name: &'static str,
    pub file_name: &'static str,
    pub description: &'static str,
    contents: &'static str,
}

pub struct ResolvedPreset {
    pub name: &'static str,
    pub path: PathBuf,
}

const BUNDLED_PRESETS: &[BundledPreset] = &[
    BundledPreset {
        name: "default",
        file_name: "default.toml",
        description: "Baseline CloakPipe configuration",
        contents: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/presets/default.toml")),
    },
    BundledPreset {
        name: "dpdp",
        file_name: "dpdp.toml",
        description: "India DPDP 2023 privacy preset",
        contents: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/presets/dpdp.toml")),
    },
    BundledPreset {
        name: "gdpr",
        file_name: "gdpr.toml",
        description: "EU GDPR privacy preset",
        contents: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/presets/gdpr.toml")),
    },
    BundledPreset {
        name: "hipaa",
        file_name: "hipaa.toml",
        description: "US HIPAA healthcare preset",
        contents: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/presets/hipaa.toml")),
    },
    BundledPreset {
        name: "pci-dss",
        file_name: "pci-dss.toml",
        description: "PCI-DSS payment-card preset",
        contents: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/presets/pci-dss.toml")),
    },
    BundledPreset {
        name: "minimal",
        file_name: "minimal.toml",
        description: "Minimal high-confidence structured-PII preset",
        contents: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/presets/minimal.toml")),
    },
];

pub fn bundled_presets() -> &'static [BundledPreset] {
    BUNDLED_PRESETS
}

pub fn installed_preset_dir() -> Result<PathBuf> {
    Ok(cloakpipe_config_home()?.join(PRESET_INSTALL_SUBDIR))
}

pub fn install_bundled_presets() -> Result<PathBuf> {
    let preset_dir = installed_preset_dir()?;
    fs::create_dir_all(&preset_dir)
        .with_context(|| format!("Cannot create preset directory: {}", preset_dir.display()))?;

    for preset in BUNDLED_PRESETS {
        let path = preset_dir.join(preset.file_name);
        if !path.exists() {
            fs::write(&path, preset.contents).with_context(|| {
                format!(
                    "Cannot install preset '{}' to {}",
                    preset.name,
                    path.display()
                )
            })?;
        }
    }

    Ok(preset_dir)
}

pub fn resolve_installed_preset(config_path: &str) -> Result<Option<ResolvedPreset>> {
    let path = Path::new(config_path);
    if path.components().count() != 1 {
        return Ok(None);
    }

    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return Ok(None);
    };

    let Some(preset) = find_bundled_preset(file_name) else {
        return Ok(None);
    };

    let preset_dir = install_bundled_presets()?;
    Ok(Some(ResolvedPreset {
        name: preset.name,
        path: preset_dir.join(preset.file_name),
    }))
}

fn find_bundled_preset(input: &str) -> Option<&'static BundledPreset> {
    let stem = input.strip_suffix(".toml").unwrap_or(input);

    BUNDLED_PRESETS.iter().find(|preset| {
        preset.name.eq_ignore_ascii_case(stem) || preset.file_name.eq_ignore_ascii_case(input)
    })
}

fn cloakpipe_config_home() -> Result<PathBuf> {
    if let Some(dir) = env::var_os(CLOAKPIPE_CONFIG_HOME_ENV) {
        return Ok(PathBuf::from(dir));
    }

    #[cfg(windows)]
    {
        if let Some(appdata) = env::var_os("APPDATA") {
            return Ok(PathBuf::from(appdata).join("cloakpipe"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = home_dir_os() {
            return Ok(PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("cloakpipe"));
        }
    }

    if let Some(xdg_config_home) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(xdg_config_home).join("cloakpipe"));
    }

    if let Some(home) = home_dir_os() {
        return Ok(PathBuf::from(home).join(".config").join("cloakpipe"));
    }

    bail!(
        "Cannot determine a CloakPipe config directory. Set {} to a writable directory.",
        CLOAKPIPE_CONFIG_HOME_ENV
    )
}

fn home_dir_os() -> Option<std::ffi::OsString> {
    env::var_os("HOME")
}

#[cfg(test)]
mod tests {
    use super::find_bundled_preset;

    #[test]
    fn bundled_preset_lookup_accepts_stem_and_filename() {
        assert_eq!(
            find_bundled_preset("dpdp").map(|preset| preset.file_name),
            Some("dpdp.toml")
        );
        assert_eq!(
            find_bundled_preset("pci-dss.toml").map(|preset| preset.name),
            Some("pci-dss")
        );
        assert!(find_bundled_preset("unknown").is_none());
    }
}
