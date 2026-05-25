<!-- markdownlint-disable-file -->

# Task Details: Missing Credentials And PII Coverage

## Research Reference

**Source Research**: #file:../research/20260525-missing-creds-pii-coverage-research.md

## Phase 1: Establish Coverage Baseline

### Task 1.1: Document missed PII and placeholder categories in tests

Create a regression inventory in the test layer that lists source values which must not appear after masking and placeholder families that should not appear under the default similar strategy.

- **Files**:
  - crates/cloakpipe-core/tests/integration.rs - Add focused core regression tests for detector and similar replacement behavior.
  - crates/cloakpipe-cli/tests/test_scan.rs - Add or extend CLI scan regression coverage for the synthetic sample.
- **Success**:
  - Tests enumerate demographics, authentication credentials, device IDs, account data, card data, narrative names, repeated NER-tail entities, and placeholder families.
  - The inventory categorizes misses as person, demographic, credential, device identifier, account/card data, organization, location, date/amount/percentage, URL/email/phone/IP, and healthcare/business identifiers.
- **Research References**:
  - #file:../research/20260525-missing-creds-pii-coverage-research.md (Lines 28-42) - Current CLI findings, missed values, and placeholder families.
  - #file:../research/20260525-missing-creds-pii-coverage-research.md (Lines 51-60) - External category basis for card data, NPI, passwords, PINs, and IMEI.
- **Dependencies**:
  - Existing sample fixture at assets/example.md.
  - Existing masked-output and mapping symptoms documented in research.

## Phase 2: Replace Placeholder Custom Fakes With Similar Values

### Task 2.1: Extend custom similar-value generators

Modify `format_preserving::generate_similar` so known custom categories produce plausible fake values instead of falling through to token-style placeholders.

- **Files**:
  - crates/cloakpipe-core/src/format_preserving.rs - Add helpers for `ID_NUMBER`, `LICENSE_NUMBER`, `IBAN`, `ROUTING_NUMBER`, `SWIFT_CODE`, `ISIN`, `ACCOUNT_NUMBER`, `USERNAME`, `DEVICE_ID`, `PIN`, `CREDIT_CARD_ISSUER`, and card verification categories.
- **Success**:
  - `MRN-2026-443821` masks as an MRN-shaped fake, not `ID_NUMBER-001`.
  - `CRC-1330841`, `#OR-48291`, and `NPI 1184729934` keep license-like shapes with fake digits.
  - IBAN, routing, SWIFT, ISIN, username, account number, IMEI/device ID, PIN, and CVV/card verification values keep valid-looking structures while changing the original value.
  - Luhn-style helpers are reused or mirrored for card numbers and IMEI check digits where applicable.
- **Research References**:
  - #file:../research/20260525-missing-creds-pii-coverage-research.md (Lines 21-24) - Similar masking currently falls back to placeholder generation for unknown custom categories.
  - #file:../research/20260525-missing-creds-pii-coverage-research.md (Lines 79-102) - Existing generator dispatch style and shape-based test pattern.
  - #file:../research/20260525-missing-creds-pii-coverage-research.md (Lines 128-132) - Required generator categories and regression expectations.
- **Dependencies**:
  - Task 1.1 inventory.
  - Existing `fake_credit_card`, `luhn_check_digit`, `fake_like_char`, and seeded helper functions.

### Task 2.2: Add generator unit tests

Add unit tests in `format_preserving.rs` for each newly supported custom category.

- **Files**:
  - crates/cloakpipe-core/src/format_preserving.rs - Add tests near existing generator tests.
- **Success**:
  - Tests assert original values are not returned.
  - Tests assert replacements match expected category shapes, not fixed seed outputs.
  - Tests cover at least ID number, license number, NPI, IBAN, routing number, SWIFT/BIC, ISIN, account number, username, IMEI/device ID, PIN, CVV, and card issuer.
- **Research References**:
  - #file:../research/20260525-missing-creds-pii-coverage-research.md (Lines 97-102) - Shape-based test example.
  - #file:../research/20260525-missing-creds-pii-coverage-research.md (Lines 62-67) - Rust testing and convention guidance.
- **Dependencies**:
  - Task 2.1 implementation.

## Phase 3: Detect Missed Structured PII And Credentials

### Task 3.1: Add deterministic pattern rules for structured misses

Extend `PatternDetector` with high-confidence contextual rules for structured PII that should not depend on NER.

- **Files**:
  - crates/cloakpipe-core/src/detector/patterns.rs - Add rules for contextual password, PIN, IMEI/device ID, account numbers, credit card numbers, card issuer notes, CVV notes, age/sex/gender labels if treated as sensitive, and doctor/provider names when practical via NER fallback rather than broad regex.
- **Success**:
  - `TempPass!2026`, `356938035643809`, `4455667788990011`, `7788990011223344`, `4111111111111111`, `5555555555554444`, `Visa`, `321`, repeated PIN/password/CVV/card issuer lines, and labeled demographics are detected.
  - New pattern categories use `EntityCategory::Custom(String)` names when no top-level category exists.
  - Regexes avoid broad false positives by using labels or strong structure, especially for age, PIN, CVV, and account numbers.
- **Research References**:
  - #file:../research/20260525-missing-creds-pii-coverage-research.md (Lines 15-20) - Existing detector coverage and DistilBERT category mapping.
  - #file:../research/20260525-missing-creds-pii-coverage-research.md (Lines 34-38) - Missed credentials, account data, demographics, and tail values.
  - #file:../research/20260525-missing-creds-pii-coverage-research.md (Lines 46-60) - Regex, PCI, NPI, and IMEI external standards.
- **Dependencies**:
  - Task 1.1 inventory.

### Task 3.2: Improve phone detection behavior for split spans

Prevent NER from producing partial phone fragments such as `212` and `555-0176` for one emergency phone number.

- **Files**:
  - crates/cloakpipe-core/src/detector/patterns.rs - Ensure robust phone pattern coverage is available when `phone_numbers` is enabled.
  - crates/cloakpipe-core/src/detector/distilbert_pii.rs or crates/cloakpipe-core/src/detector/mod.rs - Filter or merge implausible partial phone spans emitted by NER.
  - cloakpipe.toml and bundled presets if policy intent is to enable phone masking by default for sample workflows.
- **Success**:
  - Emergency phone values are masked as complete phone-shaped values.
  - Bare three-digit fragments are not emitted as standalone phone numbers.
  - Existing IP and Aadhaar precedence is preserved.
- **Research References**:
  - #file:../research/20260525-missing-creds-pii-coverage-research.md (Lines 30-33) - Current split emergency-phone behavior.
  - #file:../research/20260525-missing-creds-pii-coverage-research.md (Lines 126-132) - Technical requirement for phone handling.
- **Dependencies**:
  - Task 3.1 detector updates.

## Phase 4: Detect Long-Document NER Tail Entities

### Task 4.1: Chunk DistilBERT NER with source-offset preservation

Update DistilBERT detection to process long input in chunks while preserving byte offsets for returned entities.

- **Files**:
  - crates/cloakpipe-core/src/detector/distilbert_pii.rs - Add chunking around tokenizer/session execution or factor single-chunk inference into a helper that accepts a base offset.
- **Success**:
  - Narrative and NER-focused tail values after the current model window are detected.
  - Returned offsets still slice the original source text correctly.
  - Adjacent person merge behavior continues to work within chunks and does not create invalid cross-chunk spans.
- **Research References**:
  - #file:../research/20260525-missing-creds-pii-coverage-research.md (Lines 34-38) - Tail values currently visible in masked output.
  - #file:../research/20260525-missing-creds-pii-coverage-research.md (Lines 126-132) - Requirement to chunk long-document NER.
  - #file:../research/20260525-missing-creds-pii-coverage-research.md (Lines 134-145) - Recommended approach and success criteria.
- **Dependencies**:
  - Existing DistilBERT model assets.
  - Existing `DetectedEntity` byte-offset contract.

### Task 4.2: Add NER-tail regression tests

Add tests that prove repeated names, organizations, addresses, username, DOB, account name, password, PIN, card issuer, and CVV in the sample tail are masked.

- **Files**:
  - crates/cloakpipe-core/tests/integration.rs - Add a long-input or sample-based detector/masking regression test.
  - crates/cloakpipe-cli/tests/test_scan.rs - Add CLI-level sample scan assertion if runtime is acceptable.
- **Success**:
  - Regression fails on the current unchunked behavior and passes after chunking.
  - Assertions focus on no original sensitive values remaining and no placeholder families for similar-format custom categories.
- **Research References**:
  - #file:../research/20260525-missing-creds-pii-coverage-research.md (Lines 34-42) - Current missed tail values and placeholders.
  - #file:../research/20260525-missing-creds-pii-coverage-research.md (Lines 140-145) - Implementation guidance and verification commands.
- **Dependencies**:
  - Task 4.1 chunking.

## Dependencies

- Rust toolchain and existing workspace Cargo configuration.
- DistilBERT model assets under models/distilbert-pii for NER-backed tests.
- Existing `cloakpipe scan` CLI behavior and sample fixture.

## Success Criteria

- `cargo test -p cloakpipe-core` passes.
- `cargo test -p cloakpipe-cli test_scan` passes or any intentionally skipped model-dependent test is clearly justified.
- `cloakpipe scan assets/example.md` produces masked output without the documented source PII leaks.
- Default similar masking no longer emits `ID_NUMBER-*`, `LICENSE_NUMBER-*`, `IBAN-*`, `ROUTING_NUMBER-*`, `SWIFT_CODE-*`, or `ISIN-*` for those detected categories.