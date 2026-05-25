<!-- markdownlint-disable-file -->

# Changes: Missing Credentials And PII Coverage

## 2026-05-25

- Created implementation tracking file.
- Added similar-value generators for custom credential, banking, account, device, PIN, CVV, issuer, age, and demographic categories so default similar masking no longer falls back to targeted placeholder families.
- Added capture-aware regex pattern rules for contextual credentials, PINs, IMEIs, account/card data, issuers, CVVs, usernames, demographics, labeled names, organizations, locations, doctor names, and narrative patient names.
- Updated DistilBERT PII detection to process long inputs in overlapping chunks while preserving source byte offsets, map CVV/issuer/account/PIN labels to more specific categories, and filter implausible short phone fragments.
- Enabled phone number detection in the sample `cloakpipe.toml` configuration so scan output masks complete phone spans.
- Tightened currency amount regex whitespace so amount masking preserves line breaks in generated scan output.
- Added generator, core integration, and CLI scan regressions covering documented leaks and placeholder families.
- Regenerated `assets/example.md-masked/example.md` and `assets/example.md-masked/vault-mappings.json` with the fixed similar masking behavior.
- Replaced built-in similar-strategy placeholders (`User-*`, `Org-*`, `Location-*`, `DATE_*`, and `PCT-*`) with plausible similar values.
- Added exported mapping restoration via `cloakpipe restore`, backed by `Rehydrator::rehydrate_from_mappings`.
- Added scan/restore regression coverage proving `assets/example.md-masked/example.md` restores exactly to `assets/example.md`.
- Expanded fake-data dictionaries to 50+ variants for domains, first names, last names, organization roots, street names, states, places, card issuers, and demographic replacement values.
- Added a regression that enforces at least 50 variants per fake-data dictionary.

## Verification

- `cargo test -p cloakpipe-core` passed.
- `cargo test -p cloakpipe-cli test_scan` passed.
- `cargo run -p cloakpipe-cli -- scan assets/example.md` completed successfully with 104 detected entities.
- `cargo run -p cloakpipe-cli -- restore assets/example.md-masked/example.md -o target/tmp/example-restored.md` followed by `diff -u assets/example.md target/tmp/example-restored.md` passed.
- Regenerated the masked sample after dictionary expansion and verified restore still matches the original sample exactly.
- Verified the generated masked sample has no matches for the documented source leak inventory or targeted placeholder families: `User-`, `Org-`, `Location-`, `DATE_`, `PCT-`, `ID_NUMBER-`, `LICENSE_NUMBER-`, `IBAN-`, `ROUTING_NUMBER-`, `SWIFT_CODE-`, and `ISIN-`.
