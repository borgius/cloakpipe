---
applyTo: '.copilot-tracking/changes/20260525-missing-creds-pii-coverage-changes.md'
---

<!-- markdownlint-disable-file -->

# Task Checklist: Missing Credentials And PII Coverage

## Overview

Fix CloakPipe masking so detected custom credentials use similar-format replacements and currently missed PII in the sample document is detected, categorized, and covered by regressions.

## Objectives

- Replace placeholder-style masks such as `ID_NUMBER-001`, `LICENSE_NUMBER-001`, `IBAN-001`, `ROUTING_NUMBER-001`, `SWIFT_CODE-001`, and `ISIN-001` with plausible similar-format fakes.
- Detect missed credentials, account/card data, demographics, device identifiers, narrative names, organizations, addresses, and NER-tail values in the synthetic medical sample.
- Add or reuse categories for all newly covered PII without breaking existing serialized category compatibility.
- Add regression coverage that verifies no documented source PII leaks and no targeted placeholder families remain under similar masking.

## Research Summary

### Project Files

- assets/example.md - Source fixture containing the original synthetic PII and credentials.
- assets/example.md-masked/example.md - Current masked artifact showing missed PII and placeholder symptoms.
- assets/example.md-masked/vault-mappings.json - Current mappings proving custom placeholder categories and unmapped source values.
- crates/cloakpipe-core/src/format_preserving.rs - Similar-value generation root cause and target for custom fake generation.
- crates/cloakpipe-core/src/detector/patterns.rs - Deterministic pattern detection target for structured misses.
- crates/cloakpipe-core/src/detector/distilbert_pii.rs - NER long-document coverage target.
- crates/cloakpipe-core/tests/integration.rs - Core regression target.
- crates/cloakpipe-cli/tests/test_scan.rs - CLI regression target.

### External References

- #file:../research/20260525-missing-creds-pii-coverage-research.md - Validated project research, CLI findings, missed-value audit, and implementation guidance.
- #fetch:https://docs.rs/regex/latest/regex/ - Rust regex API and performance guidance for deterministic pattern rules.
- #fetch:https://www.pcisecuritystandards.org/glossary/ - PCI definitions for PAN, card verification code, PIN, and authentication credentials.
- #fetch:https://www.cms.gov/regulations-and-guidance/administrative-simplification/nationalprovidentstand - NPI as a 10-position healthcare identifier.
- #fetch:https://en.wikipedia.org/wiki/International_Mobile_Equipment_Identity - IMEI format and Luhn check-digit reference.

### Standards References

- #file:../../.agents/skills/rust-best-practices/SKILL.md - Rust ownership, error handling, performance, and testing conventions.
- #file:../../.github/agents/task-researcher.agent.md - Research documentation standard used before planning.

## Implementation Checklist

### [ ] Phase 1: Establish Coverage Baseline

- [ ] Task 1.1: Document missed PII and placeholder categories in tests
  - Details: .copilot-tracking/details/20260525-missing-creds-pii-coverage-details.md (Lines 11-26)

### [ ] Phase 2: Replace Placeholder Custom Fakes With Similar Values

- [ ] Task 2.1: Extend custom similar-value generators
  - Details: .copilot-tracking/details/20260525-missing-creds-pii-coverage-details.md (Lines 30-47)

- [ ] Task 2.2: Add generator unit tests
  - Details: .copilot-tracking/details/20260525-missing-creds-pii-coverage-details.md (Lines 49-63)

### [ ] Phase 3: Detect Missed Structured PII And Credentials

- [ ] Task 3.1: Add deterministic pattern rules for structured misses
  - Details: .copilot-tracking/details/20260525-missing-creds-pii-coverage-details.md (Lines 67-82)

- [ ] Task 3.2: Improve phone detection behavior for split spans
  - Details: .copilot-tracking/details/20260525-missing-creds-pii-coverage-details.md (Lines 84-100)

### [ ] Phase 4: Detect Long-Document NER Tail Entities

- [ ] Task 4.1: Chunk DistilBERT NER with source-offset preservation
  - Details: .copilot-tracking/details/20260525-missing-creds-pii-coverage-details.md (Lines 104-120)

- [ ] Task 4.2: Add NER-tail regression tests
  - Details: .copilot-tracking/details/20260525-missing-creds-pii-coverage-details.md (Lines 122-136)

## Dependencies

- Rust toolchain and current Cargo workspace.
- Existing `regex` crate usage in `cloakpipe-core`.
- DistilBERT model assets under `models/distilbert-pii` for model-backed verification.
- Existing synthetic sample document and CLI scan workflow.

## Success Criteria

- `cargo test -p cloakpipe-core` passes.
- `cargo test -p cloakpipe-cli test_scan` passes or any model-dependent skip is explicitly justified.
- `cloakpipe scan assets/example.md` succeeds and no documented source PII remains in the masked output.
- Similar masking emits plausible structured replacements instead of targeted placeholder families for detected custom credential categories.
