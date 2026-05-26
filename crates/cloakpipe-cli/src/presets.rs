use anyhow::{Context, Result};
use cloakpipe_core::paths;
use std::{
    fs,
    path::{Path, PathBuf},
};

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
    paths::global_policies_dir()
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
