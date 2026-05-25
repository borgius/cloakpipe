//! Integration tests for the CloakPipe core pipeline.

use cloakpipe_core::{
    config::{
        CloakPipeConfig, CustomConfig, CustomPattern, DetectionConfig, NerConfig, OverrideConfig,
    },
    detector::Detector,
    rehydrator::Rehydrator,
    replacer::Replacer,
    vault::Vault,
    EntityCategory, MaskingStrategy,
};
use std::{fs, path::Path};

fn test_detection_config() -> DetectionConfig {
    DetectionConfig {
        secrets: true,
        financial: true,
        dates: true,
        emails: true,
        phone_numbers: false,
        ip_addresses: true,
        urls_internal: true,
        ner: NerConfig::default(),
        custom: CustomConfig::default(),
        overrides: OverrideConfig::default(),
        resolver: Default::default(),
    }
}

fn load_policy_config(name: &str) -> CloakPipeConfig {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../policies")
        .join(name);
    let content = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read policy {}: {}", path.display(), err));
    toml::from_str(&content)
        .unwrap_or_else(|err| panic!("failed to parse policy {}: {}", path.display(), err))
}

fn has_custom_category(entities: &[cloakpipe_core::DetectedEntity], category: &str) -> bool {
    entities
        .iter()
        .any(|entity| matches!(&entity.category, EntityCategory::Custom(name) if name == category))
}

fn load_workspace_config() -> CloakPipeConfig {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cloakpipe.toml");
    let content = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read config {}: {}", path.display(), err));
    toml::from_str(&content)
        .unwrap_or_else(|err| panic!("failed to parse config {}: {}", path.display(), err))
}

#[test]
fn test_detect_email() {
    let detector = Detector::from_config(&test_detection_config()).unwrap();
    let entities = detector
        .detect("Contact alice@example.com for details")
        .unwrap();
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].original, "alice@example.com");
    assert_eq!(entities[0].category, EntityCategory::Email);
}

#[test]
fn test_detect_aws_key() {
    let detector = Detector::from_config(&test_detection_config()).unwrap();
    let entities = detector.detect("Key: AKIAIOSFODNN7EXAMPLE").unwrap();
    assert!(entities
        .iter()
        .any(|e| e.category == EntityCategory::Secret));
}

#[test]
fn test_detect_ip_address() {
    let detector = Detector::from_config(&test_detection_config()).unwrap();
    let entities = detector.detect("Server at 192.168.1.100 is down").unwrap();
    assert!(entities
        .iter()
        .any(|e| e.category == EntityCategory::IpAddress));
}

#[test]
fn test_detect_currency_amount() {
    let detector = Detector::from_config(&test_detection_config()).unwrap();
    let entities = detector.detect("Revenue was $1.2M this quarter").unwrap();
    assert!(entities
        .iter()
        .any(|e| e.category == EntityCategory::Amount));
}

#[test]
fn test_detect_percentage() {
    let detector = Detector::from_config(&test_detection_config()).unwrap();
    let entities = detector
        .detect("Growth rate: 15.3% year-over-year")
        .unwrap();
    assert!(entities
        .iter()
        .any(|e| e.category == EntityCategory::Percentage));
}

#[test]
fn test_detect_fiscal_date() {
    let detector = Detector::from_config(&test_detection_config()).unwrap();
    let entities = detector.detect("Results for Q3 2025 are out").unwrap();
    assert!(entities.iter().any(|e| e.category == EntityCategory::Date));
}

#[test]
fn test_disable_financial_and_dates() {
    let config = DetectionConfig {
        secrets: false,
        financial: false,
        dates: false,
        emails: false,
        phone_numbers: false,
        ip_addresses: false,
        urls_internal: false,
        ner: NerConfig::default(),
        custom: CustomConfig::default(),
        overrides: OverrideConfig::default(),
        resolver: Default::default(),
    };

    let detector = Detector::from_config(&config).unwrap();
    let entities = detector
        .detect("Revenue was $1.2M on March 31, 2026 with 15% growth in Q3 2025")
        .unwrap();

    assert!(
        entities.is_empty(),
        "financial/date entities should be disabled, got: {entities:?}"
    );
}

#[test]
fn test_detect_internal_url() {
    let detector = Detector::from_config(&test_detection_config()).unwrap();
    let entities = detector
        .detect("Check https://internal.corp.com/api/status")
        .unwrap();
    assert!(entities.iter().any(|e| e.category == EntityCategory::Url));
}

#[test]
fn test_detect_jwt() {
    let detector = Detector::from_config(&test_detection_config()).unwrap();
    let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
    let entities = detector.detect(&format!("Token: {}", jwt)).unwrap();
    assert!(entities
        .iter()
        .any(|e| e.category == EntityCategory::Secret));
}

#[test]
fn test_custom_pattern() {
    let config = DetectionConfig {
        secrets: false,
        financial: false,
        dates: false,
        emails: false,
        phone_numbers: false,
        ip_addresses: false,
        urls_internal: false,
        ner: NerConfig::default(),
        custom: CustomConfig {
            patterns: vec![CustomPattern {
                name: "project_codename".into(),
                regex: r"Project\s+(Alpha|Beta|Gamma)".into(),
                category: "PROJECT".into(),
            }],
        },
        overrides: OverrideConfig::default(),
        resolver: Default::default(),
    };
    let detector = Detector::from_config(&config).unwrap();
    let entities = detector.detect("Working on Project Alpha").unwrap();
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].original, "Project Alpha");
}

#[test]
fn test_preserve_list() {
    let config = DetectionConfig {
        secrets: false,
        financial: false,
        dates: false,
        emails: true,
        phone_numbers: false,
        ip_addresses: false,
        urls_internal: false,
        ner: NerConfig::default(),
        custom: CustomConfig::default(),
        overrides: OverrideConfig {
            preserve: vec!["public@example.com".into()],
            force: vec![],
        },
        resolver: Default::default(),
    };
    let detector = Detector::from_config(&config).unwrap();
    let entities = detector
        .detect("Contact public@example.com or private@secret.com")
        .unwrap();
    // public@example.com should be preserved (not detected)
    assert!(entities.iter().all(|e| e.original != "public@example.com"));
    assert!(entities.iter().any(|e| e.original == "private@secret.com"));
}

#[test]
fn test_bundled_policy_files_parse_and_build_detectors() {
    for name in [
        "default.toml",
        "dpdp.toml",
        "gdpr.toml",
        "hipaa.toml",
        "pci-dss.toml",
        "minimal.toml",
    ] {
        let config = load_policy_config(name);
        Detector::from_config(&config.detection)
            .unwrap_or_else(|err| panic!("failed to build detector for {name}: {err}"));
    }
}

#[test]
fn test_dpdp_policy_detects_india_specific_identifiers() {
    let config = load_policy_config("dpdp.toml");
    let detector = Detector::from_config(&config.detection).unwrap();
    let entities = detector
        .detect(
            "Send funds to rajesh@okicici and invoice GSTIN 27AAPFU0939F1ZV. \
             Beneficiary account number 123456789012.",
        )
        .unwrap();

    assert!(has_custom_category(&entities, "UPI_ID"));
    assert!(has_custom_category(&entities, "GSTIN"));
    assert!(has_custom_category(&entities, "BANK_ACCOUNT"));
}

#[test]
fn test_hipaa_policy_detects_healthcare_identifiers() {
    let config = load_policy_config("hipaa.toml");
    let detector = Detector::from_config(&config.detection).unwrap();
    let entities = detector
        .detect(
            "Patient MRN: MRN-2024-99187, NPI number 1548293076, DEA AB1234567, \
             ICD-10 E11.9.",
        )
        .unwrap();

    assert!(has_custom_category(&entities, "MRN"));
    assert!(has_custom_category(&entities, "NPI"));
    assert!(has_custom_category(&entities, "DEA"));
    assert!(has_custom_category(&entities, "ICD10"));
}

#[test]
fn test_pci_policy_detects_cardholder_data() {
    let config = load_policy_config("pci-dss.toml");
    let detector = Detector::from_config(&config.detection).unwrap();
    let entities = detector
        .detect("Card number 4111 1111 1111 1111, exp 12/29, CVV 123")
        .unwrap();

    assert!(has_custom_category(&entities, "CREDIT_CARD"));
    assert!(has_custom_category(&entities, "CARD_EXPIRY"));
    assert!(has_custom_category(&entities, "CARD_VERIFICATION_CODE"));
}

#[test]
fn test_detects_structured_sample_credentials_and_demographics() {
    let config = load_workspace_config();
    let detector = Detector::from_config(&config.detection).unwrap();
    let input =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/example.md"))
            .unwrap();
    let entities = detector.detect(&input).unwrap();

    for expected in [
        "TempPass!2026",
        "4821",
        "356938035643809",
        "4455667788990011",
        "7788990011223344",
        "4111111111111111",
        "5555555555554444",
        "Visa",
        "321",
        "female",
        "woman",
        "42",
        "Avery Collins",
        "Dr. Elena Morris",
        "Northwind Community Health",
        "Cedar Ridge Family Medicine",
        "Meridian Harbor Insurance",
        "1842 Willow Creek Drive",
        "Apt 5B",
        "Fairview",
        "Oregon",
        "Jefferson",
        "97035",
    ] {
        assert!(
            entities.iter().any(|entity| entity.original == expected),
            "expected detector to find {expected}"
        );
    }
}

#[test]
fn test_similar_masking_removes_structured_sample_leaks_and_placeholders() {
    let config = load_workspace_config();
    let detector = Detector::from_config(&config.detection).unwrap();
    let input =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/example.md"))
            .unwrap();
    let entities = detector.detect(&input).unwrap();
    let mut vault = Vault::ephemeral();
    let masked = Replacer::pseudonymize_with_strategy(
        &input,
        &entities,
        &mut vault,
        MaskingStrategy::Similar,
    )
    .unwrap()
    .text;

    let leak_inventory = [
        ("credential", "TempPass!2026"),
        ("credential", "4821"),
        ("device identifier", "356938035643809"),
        ("account/card data", "4455667788990011"),
        ("account/card data", "7788990011223344"),
        ("account/card data", "4111111111111111"),
        ("account/card data", "5555555555554444"),
        ("account/card data", "Visa"),
        ("account/card data", "321"),
        ("demographic", "female"),
        ("demographic", "woman"),
        ("demographic", "42"),
        ("person", "Avery Collins"),
        ("person", "Dr. Elena Morris"),
        ("organization", "Northwind Community Health"),
        ("organization", "Cedar Ridge Family Medicine"),
        ("organization", "Meridian Harbor Insurance"),
        ("location", "1842 Willow Creek Drive"),
        ("location", "Apt 5B"),
        ("location", "Fairview"),
        ("location", "Oregon"),
        ("location", "Jefferson"),
        ("location", "97035"),
        ("URL/email/phone/IP", "212-555-0176"),
        ("healthcare/business identifier", "MRN-2026-443821"),
    ];

    for (category, original) in leak_inventory {
        assert!(
            !masked.contains(original),
            "masked output leaked {category} value {original}"
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
    ] {
        assert!(
            !masked.contains(placeholder),
            "similar masking should not emit placeholder family {placeholder}"
        );
    }
}

// --- Pseudonymize + Rehydrate roundtrip ---

#[test]
fn test_pseudonymize_roundtrip() {
    let detector = Detector::from_config(&test_detection_config()).unwrap();
    let mut vault = Vault::ephemeral();

    let input = "Contact alice@example.com about the $1.2M deal in Q3 2025";
    let entities = detector.detect(input).unwrap();
    let pseudo = Replacer::pseudonymize(input, &entities, &mut vault).unwrap();

    // Pseudonymized text should not contain originals
    assert!(!pseudo.text.contains("alice@example.com"));
    assert!(!pseudo.text.contains("$1.2M"));

    // Rehydrate should recover original
    let rehydrated = Rehydrator::rehydrate(&pseudo.text, &vault).unwrap();
    assert_eq!(rehydrated.text, input);
}

#[test]
fn test_pseudonymize_consistency() {
    let detector = Detector::from_config(&test_detection_config()).unwrap();
    let mut vault = Vault::ephemeral();

    let input1 = "alice@example.com sent a message";
    let input2 = "Reply to alice@example.com";

    let e1 = detector.detect(input1).unwrap();
    let e2 = detector.detect(input2).unwrap();

    let p1 = Replacer::pseudonymize(input1, &e1, &mut vault).unwrap();
    let p2 = Replacer::pseudonymize(input2, &e2, &mut vault).unwrap();

    // Same entity should get the same token
    assert!(p1.text.contains("EMAIL_1"));
    assert!(p2.text.contains("EMAIL_1"));
}

#[test]
fn test_no_entities_passthrough() {
    let detector = Detector::from_config(&test_detection_config()).unwrap();
    let mut vault = Vault::ephemeral();

    let input = "This is a normal message with no sensitive data.";
    let entities = detector.detect(input).unwrap();
    assert!(entities.is_empty());

    let pseudo = Replacer::pseudonymize(input, &entities, &mut vault).unwrap();
    assert_eq!(pseudo.text, input);
}

#[test]
fn test_multiple_entities_same_category() {
    let detector = Detector::from_config(&test_detection_config()).unwrap();
    let mut vault = Vault::ephemeral();

    let input = "Send to alice@a.com and bob@b.com";
    let entities = detector.detect(input).unwrap();
    let pseudo = Replacer::pseudonymize(input, &entities, &mut vault).unwrap();

    assert!(pseudo.text.contains("EMAIL_1"));
    assert!(pseudo.text.contains("EMAIL_2"));
    assert!(!pseudo.text.contains("alice@a.com"));
    assert!(!pseudo.text.contains("bob@b.com"));
}

// --- Streaming rehydration ---

#[test]
fn test_streaming_rehydration_complete_token() {
    let mut vault = Vault::ephemeral();
    vault.get_or_create("Acme Corp", &EntityCategory::Organization);

    let mut buffer = String::new();
    let (output, matched) =
        Rehydrator::rehydrate_chunk("The company ORG_1 reported", &mut buffer, &vault).unwrap();
    assert!(matched);
    assert!(output.contains("Acme Corp"));
}
