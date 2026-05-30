# Bundled policy presets

These files are **full `cloakpipe.toml`-compatible configs**. Pick one with:

```bash
cloakpipe --config policies/<name>.toml start llm-proxy
```

To customize a policy interactively, edit the active config:

```bash
cloakpipe policy edit
cloakpipe --config dpdp.toml policy edit
```

When you pass a bundled preset name such as `dpdp.toml`, CloakPipe edits the installed user copy under `CLOAKPIPE_CONFIG_HOME/policies` instead of changing the bundled source file. The editor covers detection toggles, replacement strategy, NER settings, custom regex patterns, preserve values, and force values.

Preset installation now uses the global CloakPipe home: `~/.cloakpipe/policies` by default, or `CLOAKPIPE_HOME/policies` when `CLOAKPIPE_HOME` is set. `CLOAKPIPE_CONFIG_HOME` remains supported for existing setups.

If `--config` is omitted, CloakPipe searches from the current directory upward for `cloakpipe.toml`, then `cloackpipe.toml`, before falling back to `~/.cloakpipe/cloakpipe.toml`.

## Presets

- `default.toml` — baseline config already shipped with the repo
- `minimal.toml` — lowest-noise preset; structured identifiers only, NER off
- `dpdp.toml` — India-focused preset with UPI, GSTIN, IFSC, and contextual bank-account detection
- `gdpr.toml` — broad personal-data preset with IP, URL, IBAN, and VAT coverage
- `hipaa.toml` — PHI-focused preset with MRN, NPI, DEA, ICD-10, and insurance-member patterns
- `pci-dss.toml` — cardholder-data preset with PAN, expiry, CVV, and track-data-like patterns

## Notes

- These presets define **technical masking defaults**, not legal certification.
- Optional NER is left disabled in the bundled files because the repository does not ship model assets by default.
- If you add a local model, you can enable `[detection.ner]` in any preset for names, addresses, and organization detection.
