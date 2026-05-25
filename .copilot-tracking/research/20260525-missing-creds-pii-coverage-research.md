<!-- markdownlint-disable-file -->

# Task Research Notes: Missing Credentials And PII Coverage

## Research Executed

### File Analysis

- assets/example.md
  - Source synthetic medical report includes names, demographics, credentials, identifiers, money, dates, URLs, IPs, billing data, secrets, and repeated NER-focused values.
- assets/example.md-masked/example.md
  - Current masked output still leaks several values and uses token-like placeholders for custom identifiers that should be similar-format replacements.
- assets/example.md-masked/vault-mappings.json
  - Mapping confirms `ID_NUMBER-*`, `LICENSE_NUMBER-*`, `IBAN-*`, `ROUTING_NUMBER-*`, `SWIFT_CODE-*`, and `ISIN-*` were emitted as placeholders, while several originals never received mappings.
- crates/cloakpipe-core/src/detector/patterns.rs
  - Pattern detector covers secrets, email, IP, SSN, Aadhaar, PAN, prefixed IDs, license numbers, phone when enabled, and URLs.
- crates/cloakpipe-core/src/detector/financial.rs
  - Financial detector covers currency amounts, percentages, fiscal periods, ISO dates, slash dates, and month-name dates.
- crates/cloakpipe-core/src/detector/distilbert_pii.rs
  - DistilBERT maps labels for person, organization, location, date, email, phone, URL, SSN, credit card, account number, password, PIN, amount, username, device ID, age, gender, and sex.
- crates/cloakpipe-core/src/format_preserving.rs
  - Similar masking currently has custom handling only for SSN and credit card under `EntityCategory::Custom`; all other custom categories fall back to `generate`, which emits placeholder tokens for unknown custom names.
- crates/cloakpipe-core/src/vault.rs
  - `Vault::get_or_create_similar` routes every similar replacement through `format_preserving::generate_similar`, so fixing generators addresses CLI, proxy, and any caller using the default similar strategy.
- cloakpipe.toml
  - Active config enables secrets, financial, dates, emails, IPs, URLs, and DistilBERT NER at threshold 0.4; `phone_numbers` is false, and custom config adds SWIFT, ISIN, IBAN, and routing number patterns.

### Code Search Results

- Current CLI behavior
  - `cloakpipe scan assets/example.md --detect-only` reported 72 entities with strategy `similar` and threshold 50%.
  - Detected values include the main patient names, dates, username, locations, emails, seven `ID_NUMBER` values, three `LICENSE_NUMBER` values, SSN, Aadhaar, PAN, money, percentages, URLs, IPs, IBANs, routing numbers, SWIFT codes, ISINs, and eight secrets.
  - The emergency phone was split into `212` and `555-0176` by NER because pattern phone detection is disabled in config.
- Missed values visible in masked output
  - Demographics: `Jordan`, `female`, `woman`, `42`.
  - Authentication and credentials: `TempPass!2026`, `4821` as a PIN-shaped value, `356938035643809`, account numbers `4455667788990011` and `7788990011223344`, credit cards `4111111111111111` and `5555555555554444`, issuer `Visa`, CVV `321`, repeated password/PIN/card issuer/CVV in the NER-focused tail.
  - People and organizations in narrative/tail: `Dr. Elena Morris`, repeated `Avery Collins`, `Avery`, `Collins`, `Jordan`, `Northwind Community Health`, `Cedar Ridge Family Medicine`, `Meridian Harbor Insurance`.
  - Addresses and locations in narrative/tail: `1842 Willow Creek Drive`, `Apt 5B`, `Fairview`, `Oregon`, `Jefferson`, `97035` when repeated near the end.
- Placeholder values that should become similar-format fakes
  - `ID_NUMBER-001..007` for MRN/EMP/POL/CLM/REF/MBR/TN style IDs.
  - `LICENSE_NUMBER-001..003` for CRC, state license, and NPI values.
  - `IBAN-001..002`, `ROUTING_NUMBER-001..002`, `SWIFT_CODE-001..002`, `ISIN-001..002`, `USERNAME-001`, `PCT-*`, `DATE_*`, and `Location-*` are also semantically weaker than similar strategy implies.

### External Research

- #fetch:https://docs.rs/regex/latest/regex/ - Rust regex documentation
  - `Regex::find_iter` returns matched byte offsets; current detector pattern code already uses this API.
  - Rust regex supports raw string literals, named captures, ASCII digit classes, and case-insensitive flags.
  - The crate guarantees finite-automata searching without unbounded backtracking and documents worst-case `O(m * n)` for single searches.
  - Documentation recommends avoiding regex recompilation in loops; current detector compiles rules once in `PatternDetector::new`.
- #fetch:https://www.pcisecuritystandards.org/glossary/ - PCI SSC glossary
  - Cardholder data includes at minimum the full PAN and can include cardholder name.
  - Sensitive authentication data includes card verification codes and PINs.
  - Password/passphrase and PIN are authentication factors/credentials and should be treated as sensitive.
- #fetch:https://www.cms.gov/regulations-and-guidance/administrative-simplification/nationalprovidentstand - CMS NPI standard
  - NPI is a HIPAA Administrative Simplification Standard and a unique identification number for covered health care providers.
  - NPI is a 10-position numeric identifier.
- #fetch:https://en.wikipedia.org/wiki/International_Mobile_Equipment_Identity - IMEI format reference
  - IMEI is a 15-digit numeric identifier for mobile devices.
  - IMEI includes a Luhn check digit; generated fakes should remain 15 digits and can use the existing Luhn helper pattern already present for credit cards.

### Project Conventions

- Standards referenced: .agents/skills/rust-best-practices/SKILL.md
  - Prefer `&str` parameters, avoid unnecessary clones, return `Result` for fallible operations, and add focused tests for behavior.
- Instructions followed: .github/agents/task-researcher.agent.md
  - Research is documented only in `.copilot-tracking/research/` and planning is based on verified tool findings.

## Key Discoveries

### Project Structure

Detection and masking are centralized in `cloakpipe-core`. The CLI scan command constructs a `Detector`, gathers entities, and masks through `Replacer`, which in turn delegates to `Vault` and `format_preserving` based on `MaskingStrategy`. Fixes in detector patterns and format generators will flow into CLI scan output and other core users.

### Implementation Patterns

`PatternDetector::new` builds a vector of `PatternRule` values with compiled `Regex` instances and category metadata. `PatternDetector::detect` loops over `find_iter`, creates `DetectedEntity` records with byte offsets, and returns a flat vector later merged by `Detector::deduplicate_spans`. `generate_similar` is the default similar-strategy formatting hook and should be extended instead of bypassing the vault.

### Complete Examples

```rust
// Existing detector pattern style to follow.
rules.push(PatternRule {
    regex: Regex::new(r"\b(?i:EMP|INS|WF|FM|ANT|SH|MRN|TN|CP|HR|POL|CLM|REF|ACCT|MBR|HO)[-–]\d[\w-]{3,}\b")?,
    category: EntityCategory::Custom("ID_NUMBER".into()),
    _name: "prefixed_id".into(),
});

// Existing custom similar-dispatch style to extend.
EntityCategory::Custom(name) => match name.to_uppercase().as_str() {
    "SSN" | "SOCIAL_SECURITY_NUMBER" => fake_ssn(id),
    "CREDIT_CARD" | "CREDIT_CARD_NUMBER" | "PAYMENT_CARD" => fake_credit_card(original, id),
    _ => generate(original, category, id),
},
```

```rust
// New tests should assert both masking and shape, not one specific seed value.
let fake = generate_similar("MRN-2026-443821", &EntityCategory::Custom("ID_NUMBER".into()), 1);
assert_ne!(fake, "MRN-2026-443821");
assert!(regex::Regex::new(r"^MRN-\d{4}-\d{6}$").unwrap().is_match(&fake));
```

### API and Schema Documentation

`EntityCategory` currently has top-level variants for person, organization, location, amount, percentage, date, email, phone, IP, secret, URL, project, business, infra, and `Custom(String)`. The quickest compatible path is to use custom category strings for new structured detections, then generate category-specific similar values in `format_preserving`. New top-level enum variants are only needed if broad API consumers need compile-time category names.

### Configuration Examples

```toml
[detection]
secrets = true
financial = true
dates = true
emails = true
phone_numbers = false
ip_addresses = true
urls_internal = true

[[detection.custom.patterns]]
name = "iban"
regex = '\b[A-Z]{2}\d{2}[A-Z0-9]{4}\d{7}(?:[A-Z0-9]){0,16}\b'
category = "IBAN"
```

### Technical Requirements

- Add similar-value generators for custom categories that currently emit placeholders: `ID_NUMBER`, `LICENSE_NUMBER`, `IBAN`, `ROUTING_NUMBER`, `SWIFT_CODE`, `ISIN`, `ACCOUNT_NUMBER`, `USERNAME`, `DEVICE_ID`, and related payment categories.
- Add pattern rules for high-confidence structured misses: IMEI/device IDs, contextual passwords, PINs, patient/account numbers, credit card/PAN values with Luhn validation where practical, card issuer labels, and CVV labels.
- Fix long-document NER coverage by chunking DistilBERT input with source-offset preservation so entities after the model/tokenizer window are detected.
- Improve phone handling so disabled regex phone detection does not leave emergency phones as partial NER fragments; either enable robust phone patterns in the sample config/presets or filter partial phone spans and rely on pattern detection when phone support is enabled.
- Add regression coverage for sample-document masking that asserts no known source PII remains and no token placeholders remain for categories expected to be similar-format.

## Recommended Approach

Implement this in three phases. First, extend `format_preserving::generate_similar` with deterministic helpers for the existing custom categories so detected credentials use plausible replacements without changing public category serialization. Second, expand deterministic pattern detection for structured misses that should not rely on NER. Third, chunk DistilBERT NER over long input and add focused regression tests using the sample document and generator-level assertions.

This approach preserves the existing `EntityCategory::Custom(String)` extension point, avoids a broad serialization-breaking enum change, and fixes the root causes visible in the sample: placeholder-like custom fakes, missing high-confidence structured PII, and NER truncation on long files.

## Implementation Guidance

- **Objectives**: Replace placeholder custom masks with similar-format fakes, detect missed credentials and PII categories, and prevent long-document NER misses.
- **Key Tasks**: Extend custom fake generators, add structured pattern rules and tests, add DistilBERT chunking with offset correction, and add sample regression checks.
- **Dependencies**: Rust `regex`, existing Luhn helper pattern in `format_preserving`, current CLI scan behavior, and the DistilBERT model assets already present under `models/distilbert-pii`.
- **Success Criteria**: `cargo test -p cloakpipe-core`, `cargo test -p cloakpipe-cli test_scan`, and `cloakpipe scan assets/example.md` succeed; masked sample contains no known source PII from the audit list; custom ID/license/financial-routing placeholders are replaced with format-similar fakes.