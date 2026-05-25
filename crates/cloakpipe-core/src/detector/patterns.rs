//! Layer 1: Regex-based pattern detection for secrets, emails, IPs, etc.

use crate::{config::DetectionConfig, DetectedEntity, DetectionSource, EntityCategory};
use anyhow::Result;
use regex::Regex;

pub struct PatternDetector {
    rules: Vec<PatternRule>,
}

struct PatternRule {
    regex: Regex,
    category: EntityCategory,
    _name: String,
    value_group: Option<usize>,
}

impl PatternRule {
    fn new(regex: Regex, category: EntityCategory, name: &str) -> Self {
        Self {
            regex,
            category,
            _name: name.into(),
            value_group: None,
        }
    }

    fn with_value_group(
        regex: Regex,
        category: EntityCategory,
        name: &str,
        value_group: usize,
    ) -> Self {
        Self {
            regex,
            category,
            _name: name.into(),
            value_group: Some(value_group),
        }
    }
}

impl PatternDetector {
    pub fn new(config: &DetectionConfig) -> Result<Self> {
        let mut rules = Vec::new();

        if config.secrets {
            // AWS keys
            rules.push(PatternRule::new(
                Regex::new(r"(?i)(AKIA[0-9A-Z]{16})")?,
                EntityCategory::Secret,
                "aws_access_key",
            ));
            // OpenAI / generic API keys (sk-proj-*, sk-live-*, sk-<32+ alphanum>)
            rules.push(PatternRule::new(
                Regex::new(r"sk-(?:proj|live|test|prod)-[a-zA-Z0-9]{10,}")?,
                EntityCategory::Secret,
                "api_key_prefixed",
            ));
            rules.push(PatternRule::new(
                Regex::new(r"sk-[a-zA-Z0-9]{32,}")?,
                EntityCategory::Secret,
                "api_key_generic",
            ));
            // GitHub tokens
            rules.push(PatternRule::new(
                Regex::new(r"(?i)(ghp_[a-zA-Z0-9]{36}|gho_[a-zA-Z0-9]{36}|ghs_[a-zA-Z0-9]{36}|github_pat_[a-zA-Z0-9_]{22,})")?,
                EntityCategory::Secret,
                "github_token",
            ));
            // Connection strings
            rules.push(PatternRule::new(
                Regex::new(
                    r"(?i)(postgres(?:ql)?://[^\s]+|mysql://[^\s]+|mongodb(?:\+srv)?://[^\s]+)",
                )?,
                EntityCategory::Secret,
                "connection_string",
            ));
            // JWT tokens
            rules.push(PatternRule::new(
                Regex::new(r"eyJ[a-zA-Z0-9_-]+\.eyJ[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+")?,
                EntityCategory::Secret,
                "jwt_token",
            ));
        }

        if config.emails {
            rules.push(PatternRule::new(
                Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}")?,
                EntityCategory::Email,
                "email",
            ));
        }

        // IP addresses MUST come before phone numbers so they win dedup
        if config.ip_addresses {
            rules.push(PatternRule::new(
                Regex::new(
                    r"\b(?:(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\b",
                )?,
                EntityCategory::IpAddress,
                "ipv4",
            ));
        }

        // Identity documents MUST come before phone (Aadhaar overlaps phone pattern)
        rules.push(PatternRule::new(
            Regex::new(r"\b\d{3}-\d{2}-\d{4}\b")?,
            EntityCategory::Custom("SSN".into()),
            "ssn",
        ));
        rules.push(PatternRule::new(
            Regex::new(r"\b\d{4}\s\d{4}\s\d{4}\b")?,
            EntityCategory::Custom("AADHAAR".into()),
            "aadhaar",
        ));
        rules.push(PatternRule::new(
            Regex::new(r"\b[A-Z]{5}\d{4}[A-Z]\b")?,
            EntityCategory::Custom("PAN".into()),
            "pan",
        ));

        // Employee / member / policy IDs with common prefixes
        // Matches: EMP-2019-4471, INS-2026-78432, WF-2019-445821, FM-2026-11847, etc.
        rules.push(PatternRule::new(
            Regex::new(r"\b(?i:EMP|INS|WF|FM|ANT|SH|MRN|TN|CP|HR|POL|CLM|REF|ACCT|MBR|HO)[-–]\d[\w-]{3,}\b")?,
            EntityCategory::Custom("ID_NUMBER".into()),
            "prefixed_id",
        ));

        // License / certificate numbers with prefix (CRC-1330841, NMLS #1847293, etc.)
        rules.push(PatternRule::new(
            Regex::new(r"\b(?:CRC|NMLS|NPI|DEA|LPC|BAR|LIC)[-–#\s]*\d{4,}\b")?,
            EntityCategory::Custom("LICENSE_NUMBER".into()),
            "license_number",
        ));

        // State/professional license with hash prefix (#TX-28491, #GA-12847)
        rules.push(PatternRule::new(
            Regex::new(r"#[A-Z]{2}-\d{4,}")?,
            EntityCategory::Custom("LICENSE_NUMBER".into()),
            "state_license",
        ));

        rules.push(PatternRule::with_value_group(
            Regex::new(r"(?i)\b(?:temporary\s+password|password\s+repeat|password|passcode)\s*[:=]\s*([^\s,;]+)")?,
            EntityCategory::Secret,
            "contextual_password",
            1,
        ));
        rules.push(PatternRule::with_value_group(
            Regex::new(r"(?i)\b(?:pin\s+on\s+file|pin\s+repeat|pin)\s*[:=]\s*(\d{4,8})\b")?,
            EntityCategory::Custom("PIN".into()),
            "contextual_pin",
            1,
        ));
        rules.push(PatternRule::with_value_group(
            Regex::new(r"(?i)\b(?:device\s+imei|phone\s+imei\s+repeat|phone\s+imei|imei)\s*[:=]\s*(\d{15})\b")?,
            EntityCategory::Custom("DEVICE_ID".into()),
            "contextual_imei",
            1,
        ));
        rules.push(PatternRule::with_value_group(
            Regex::new(r"(?i)\b(?:patient\s+account\s+number|secondary\s+account\s+number|account\s+number)\s*[:=]\s*(\d{8,19})\b")?,
            EntityCategory::Custom("ACCOUNT_NUMBER".into()),
            "contextual_account_number",
            1,
        ));
        rules.push(PatternRule::with_value_group(
            Regex::new(
                r"(?i)\b(?:credit\s+card\s+\d+|card\s+number)\s*[:=]\s*((?:\d[ -]?){13,19})\b",
            )?,
            EntityCategory::Custom("CREDIT_CARD".into()),
            "contextual_credit_card",
            1,
        ));
        rules.push(PatternRule::with_value_group(
            Regex::new(r"(?i)\b(?:card\s+issuer\s+note|credit\s+card\s+issuer\s+repeat)\s*[:=]\s*(visa|mastercard|american\s+express|amex|discover)\b")?,
            EntityCategory::Custom("CREDIT_CARD_ISSUER".into()),
            "contextual_card_issuer",
            1,
        ));
        rules.push(PatternRule::with_value_group(
            Regex::new(
                r"(?i)\b(?:cvv\s+note|credit\s+card\s+cvv\s+repeat|cvv|cvc)\s*[:=]\s*(\d{3,4})\b",
            )?,
            EntityCategory::Custom("CARD_VERIFICATION_CODE".into()),
            "contextual_card_verification_code",
            1,
        ));
        rules.push(PatternRule::with_value_group(
            Regex::new(
                r"(?i)\b(?:portal\s+username|username\s+repeat)\s*[:=]\s*([a-z0-9._%+-]{3,})\b",
            )?,
            EntityCategory::Custom("USERNAME".into()),
            "contextual_username",
            1,
        ));
        rules.push(PatternRule::with_value_group(
            Regex::new(r"(?i)\b(?:age)\s*[:=]\s*(\d{1,3})\b")?,
            EntityCategory::Custom("AGE".into()),
            "contextual_age",
            1,
        ));
        rules.push(PatternRule::with_value_group(
            Regex::new(r"(?i)\b(?:sex|gender)\s*[:=]\s*(female|male|woman|man|nonbinary|transgender|intersex)\b")?,
            EntityCategory::Custom("GENDER".into()),
            "contextual_gender",
            1,
        ));
        rules.push(PatternRule::with_value_group(
            Regex::new(r"(?i)\b(?:patient|preferred\s+name|middle\s+name(?:\s+repeat)?|first\s+name|last\s+name|account\s+holder|account\s+name\s+repeat)\s*[:=]\s*([A-Z][A-Za-z]+(?:[ ]+[A-Z][A-Za-z]+){0,3})")?,
            EntityCategory::Person,
            "contextual_person_name",
            1,
        ));
        rules.push(PatternRule::new(
            Regex::new(r"\bDr\.[ ]+[A-Z][A-Za-z]+[ ]+[A-Z][A-Za-z]+\b")?,
            EntityCategory::Person,
            "doctor_name",
        ));
        rules.push(PatternRule::with_value_group(
            Regex::new(r"\b(?:evaluated|patient)\s+([A-Z][A-Za-z]+[ ]+[A-Z][A-Za-z]+)\b")?,
            EntityCategory::Person,
            "narrative_person_name",
            1,
        ));
        rules.push(PatternRule::with_value_group(
            Regex::new(r"(?i)\b(?:employer|insurer|primary\s+clinic|company\s+name\s+repeat)\s*[:=]\s*([A-Z][A-Za-z]+(?:[ ]+[A-Z][A-Za-z]+){1,5})")?,
            EntityCategory::Organization,
            "contextual_organization",
            1,
        ));
        rules.push(PatternRule::new(
            Regex::new(r"\b[A-Z][A-Za-z]+(?:[ ]+[A-Z][A-Za-z]+){1,4}[ ]+(?:Health|Insurance|Medicine|Clinic|Hospital)\b")?,
            EntityCategory::Organization,
            "organization_suffix",
        ));
        rules.push(PatternRule::with_value_group(
            Regex::new(r"(?i)\b(?:home\s+address\s+[12]|street\s+repeat|secondary\s+address\s+repeat|city|city\s+repeat|county|county\s+repeat|state|state\s+repeat|zip\s+code|zip\s+repeat)\s*[:=]\s*([A-Za-z0-9][A-Za-z0-9 .#-]{1,80})")?,
            EntityCategory::Location,
            "contextual_location",
            1,
        ));

        if config.phone_numbers {
            // Tighter phone regex: requires country code or area code pattern,
            // minimum 7 digits total, won't match bare 4-digit numbers or IPs
            rules.push(PatternRule::new(
                Regex::new(r"(?:\+[1-9]\d{0,2}[-.\s]?)?\(?\d{2,4}\)?[-.\s]?\d{3,4}[-.\s]?\d{4,}")?,
                EntityCategory::PhoneNumber,
                "phone",
            ));
        }

        // URLs: both internal and general
        if config.urls_internal {
            rules.push(PatternRule::new(
                Regex::new(
                    r"https?://[a-zA-Z0-9](?:[a-zA-Z0-9.-]*[a-zA-Z0-9])?(?::\d{1,5})?(?:/[^\s)]*)?",
                )?,
                EntityCategory::Url,
                "url",
            ));
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
                            source: DetectionSource::Pattern,
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
                    source: DetectionSource::Pattern,
                });
            }
        }
        Ok(entities)
    }
}
