//! Layer 4: User-defined TOML pattern rules.

use crate::{config::DetectionConfig, DetectedEntity, DetectionSource, EntityCategory};
use anyhow::Result;
use regex::Regex;

pub struct CustomDetector {
    rules: Vec<CustomRule>,
}

struct CustomRule {
    _name: String,
    regex: Regex,
    category: EntityCategory,
    value_group: Option<usize>,
}

impl CustomDetector {
    pub fn new(config: &DetectionConfig) -> Result<Self> {
        let mut rules = Vec::new();
        for pattern in &config.custom.patterns {
            rules.push(CustomRule {
                _name: pattern.name.clone(),
                regex: Regex::new(&pattern.regex)?,
                category: EntityCategory::Custom(pattern.category.clone()),
                value_group: pattern.value_group,
            });
        }
        Ok(Self { rules })
    }

    pub fn detect(&self, text: &str) -> Result<Vec<DetectedEntity>> {
        let mut entities = Vec::new();
        for rule in &self.rules {
            if let Some(value_group) = rule.value_group {
                for captures in rule.regex.captures_iter(text) {
                    if let Some(mat) = captures.get(value_group) {
                        entities.push(DetectedEntity {
                            original: mat.as_str().trim().to_string(),
                            start: mat.start(),
                            end: mat.end(),
                            category: rule.category.clone(),
                            confidence: 1.0,
                            source: DetectionSource::Custom,
                        });
                    }
                }
                continue;
            }

            for mat in rule.regex.find_iter(text) {
                entities.push(DetectedEntity {
                    original: mat.as_str().to_string(),
                    start: mat.start(),
                    end: mat.end(),
                    category: rule.category.clone(),
                    confidence: 1.0,
                    source: DetectionSource::Custom,
                });
            }
        }
        Ok(entities)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CustomConfig, CustomPattern, DetectionConfig, NerConfig, OverrideConfig};

    fn config_with_pattern(pattern: CustomPattern) -> DetectionConfig {
        DetectionConfig {
            secrets: false,
            financial: false,
            dates: false,
            emails: false,
            phone_numbers: false,
            ip_addresses: false,
            urls_internal: false,
            ner: NerConfig::default(),
            custom: CustomConfig {
                patterns: vec![pattern],
            },
            overrides: OverrideConfig::default(),
            resolver: Default::default(),
        }
    }

    #[test]
    fn detect_uses_value_group_when_configured() {
        let detector = CustomDetector::new(&config_with_pattern(CustomPattern {
            name: "pin".into(),
            regex: r"(?i)\bpin\s*[:=]\s*(\d{4,8})\b".into(),
            category: "PIN".into(),
            value_group: Some(1),
        }))
        .unwrap();

        let entities = detector.detect("PIN: 4821").unwrap();

        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].original, "4821");
        assert_eq!(entities[0].start, 5);
        assert_eq!(entities[0].end, 9);
        assert_eq!(entities[0].category, EntityCategory::Custom("PIN".into()));
    }
}
