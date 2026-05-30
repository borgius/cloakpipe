# CloakPipe MCP Server

CloakPipe exposes six Model Context Protocol (MCP) tools over stdio for agent integrations that need reversible privacy controls.

Use the MCP server when an agent needs to:

- pseudonymize sensitive text before it leaves the process
- inspect what CloakPipe would detect
- restore placeholders after a model responds
- change detection settings without restarting the server

This page documents the current stdio implementation in `crates/cloakpipe-mcp/src/lib.rs` and calls out the caveats that matter in practice.

## What the server exposes

The MCP server enables **tools only**. It does not publish MCP resources or prompts.

- `pseudonymize`: replace sensitive spans with stable tokens such as `EMAIL_1` and `AMOUNT_1`
- `rehydrate`: restore CloakPipe tokens back to their original values
- `detect`: run a dry scan and return the entities CloakPipe would detect
- `vault_stats`: return safe aggregate stats about the active vault
- `configure`: switch industry profiles or toggle detection categories at runtime
- `session_context`: inspect session stats, with important limitations in the current MCP path

## Starting the server

Run the server mode with the CLI. It starts the direct HTTP API surface and the stdio MCP tools without registering LLM proxy routes:

```bash
cloakpipe --config /absolute/path/to/cloakpipe.toml start server
```

You can also omit `--config`. CloakPipe searches from the current directory upward for `cloakpipe.toml`, then `cloackpipe.toml`, and then uses `~/.cloakpipe/cloakpipe.toml`.

For local development from the workspace root, you can also run:

```bash
cargo run -p cloakpipe-cli -- --config /absolute/path/to/cloakpipe.toml start server
```

A typical MCP client entry looks like this:

```json
{
  "mcpServers": {
    "cloakpipe": {
      "command": "cloakpipe",
      "args": [
        "--config",
        "/absolute/path/to/cloakpipe.toml",
        "start",
        "server"
      ],
      "env": {
        "CLOAKPIPE_VAULT_KEY": "<64-char-hex-key>"
      }
    }
  }
}
```

## Runtime requirements

### Configuration file

The CLI loads the file passed via `--config`. Missing explicit paths are errors. When `--config` is omitted, CloakPipe uses project-to-global discovery and bootstraps `~/.cloakpipe/cloakpipe.toml` if no project config exists.

### Vault key

The vault key comes from `config.vault.key_env`, which defaults to `CLOAKPIPE_VAULT_KEY`.

Important behavior:

- If `CLOAKPIPE_VAULT_KEY` is set, it must be a **64-character hex string** representing 32 bytes.
- If the variable is missing, the server still starts, but it generates an **ephemeral in-memory key** for that process.
- An ephemeral key means tokens from a previous run cannot be rehydrated after restart.

### What persists today

The MCP server opens the configured vault file, but the current stdio implementation does **not** save new mappings back to disk after tool calls.

That means:

- `pseudonymize` and `rehydrate` work together during the same MCP server process
- a pre-existing vault file can still be loaded at startup if you provide the right key
- **new mappings created through the MCP server are currently lost when the process exits**

If you depend on cross-restart persistence, treat that as a current limitation of the MCP path.

### Audit and session context

MCP tool calls emit audit metadata with `surface = "mcp"`. The MCP server also creates and uses the `global` session for direct tool calls, so `session_context` can inspect context accumulated through MCP pseudonymization.

## Response format and error handling

All six tool handlers currently return a `String`. In most MCP clients, you should expect **text content that contains JSON**, not a typed JSON result object.

### Success responses

On success, tools return pretty-printed JSON text.

### Error responses

Error handling is not fully uniform:

- most tools return plain text starting with `Error:`
- `session_context` returns a JSON error object when a specific session does not exist

For robust client code:

1. treat the tool result as text first
2. check whether it starts with `Error:`
3. otherwise parse it as JSON

## Choosing the right tool

| If you need to...                                      | Use               |
| ------------------------------------------------------ | ----------------- |
| redact a prompt before sending it elsewhere            | `pseudonymize`    |
| turn `EMAIL_1` back into the original value            | `rehydrate`       |
| review what would be caught without changing the text  | `detect`          |
| see whether the in-memory vault is filling up          | `vault_stats`     |
| switch to `legal` or disable `emails` mid-session      | `configure`       |
| inspect session stats                                  | `session_context` |

## Tool reference

### `pseudonymize`

Replace detected sensitive entities with stable token placeholders.

Use this tool before sending text to an external LLM, analytics system, or any downstream component that should not receive raw PII.

#### `pseudonymize` request

| Field  | Type     | Required | Description                         |
| ------ | -------- | -------- | ----------------------------------- |
| `text` | `string` | Yes      | Raw text to scan and pseudonymize   |

#### `pseudonymize` success response

- `text` (`string`): the pseudonymized text
- `entities_detected` (`number`): number of detected entity spans that were replaced
- `categories` (`string[]`): unique categories found in the text

Example:

```json
{
  "text": "Send AMOUNT_1 to EMAIL_1 before DATE_1.",
  "entities_detected": 3,
  "categories": ["Amount", "Date", "Email"]
}
```

#### `pseudonymize` usage tips

1. Pass the original text in `text`.
2. Send the returned `text` to the downstream model or service.
3. Keep the CloakPipe MCP server alive if you plan to call `rehydrate` later.

#### `pseudonymize` token prefixes

CloakPipe generates token-style placeholders with category prefixes such as:

- `PERSON`
- `ORG`
- `LOC`
- `AMOUNT`
- `PCT`
- `DATE`
- `EMAIL`
- `PHONE`
- `IP`
- `SECRET`
- `URL`
- `PROJECT`
- `BIZ`
- `INFRA`
- custom categories are uppercased and used as the prefix

#### `pseudonymize` caveats

- The MCP tool always uses **token masking**. It does not expose CloakPipe's format-preserving masking strategy.
- The same original value maps to the same token **within the same loaded vault state**.
- `categories` is deduplicated and its order is not guaranteed.
- This MCP tool does **not** accept a `session_id`, so the pseudonymization path is effectively stateless apart from the in-memory vault.
- If detection fails, the tool returns plain text like `Error: Detection failed: ...`.

### `rehydrate`

Restore CloakPipe placeholders such as `EMAIL_1` or `AMOUNT_2` back to their original values.

Use this after a downstream model responds with CloakPipe tokens that you want to turn back into readable text.

#### `rehydrate` request

| Field  | Type     | Required | Description                           |
| ------ | -------- | -------- | ------------------------------------- |
| `text` | `string` | Yes      | Text that contains CloakPipe tokens   |

#### `rehydrate` success response

- `text` (`string`): the restored text
- `tokens_rehydrated` (`number`): number of distinct token mappings that were applied

Example:

```json
{
  "text": "Send $500 to alice@example.com before June 1, 2026.",
  "tokens_rehydrated": 3
}
```

#### `rehydrate` usage tips

1. Keep the same MCP server process running after `pseudonymize`.
2. Pass the model output, still containing CloakPipe tokens, into `rehydrate`.
3. Use the restored `text` in your UI, logs, or final response.

#### `rehydrate` caveats

- `rehydrate` only works for tokens that exist in the active vault state.
- Unknown tokens are left unchanged.
- Tokens are replaced longest-first, so `ORG_12` is restored before `ORG_1`.
- `tokens_rehydrated` counts distinct mappings applied, not every textual occurrence.
- If rehydration fails, the tool returns plain text like `Error: Rehydrate failed: ...`.

### `detect`

Run a dry scan and return the entities CloakPipe would detect **without** replacing them.

Use this for audits, debugging, prompt reviews, or testing policy changes.

#### `detect` request

| Field  | Type     | Required | Description          |
| ------ | -------- | -------- | -------------------- |
| `text` | `string` | Yes      | Raw text to scan     |

#### `detect` success response

- `entities` (`array`): all detected entities
- `entities[].original` (`string`): the original sensitive substring
- `entities[].category` (`string`): entity category such as `Email`, `Amount`, or `Person`
- `entities[].confidence` (`number`): detection confidence
- `entities[].source` (`string`): detection layer, one of `Pattern`, `Financial`, `Ner`, or `Custom`

Representative shape:

```json
{
  "entities": [
    {
      "original": "alice@example.com",
      "category": "Email",
      "confidence": 1.0,
      "source": "Pattern"
    },
    {
      "original": "$500",
      "category": "Amount",
      "confidence": 1.0,
      "source": "Financial"
    }
  ]
}
```

#### `detect` usage tips

- Run `detect` first when you are tuning a policy and want to see what CloakPipe will catch.
- Pair it with `configure` to compare different profiles or category toggles.
- Use it for validation and debugging, not for the redacted outbound flow.

#### `detect` caveats

- `detect` returns the **raw sensitive substrings** in `original`.
- Because of that, do **not** use `detect` as a privacy-preserving substitute for `pseudonymize`.
- Results respect the current in-memory detector configuration, including any changes made through `configure`.
- If detection fails, the tool returns plain text like `Error: Detection failed: ...`.

### `vault_stats`

Return safe aggregate stats for the active vault.

Use this to confirm that mappings are being created and to inspect the mix of token categories currently stored in memory.

#### `vault_stats` request

This tool takes no parameters.

#### `vault_stats` success response

- `total_mappings` (`number`): number of mappings currently tracked in the vault
- `categories` (`object`): per-prefix counts such as `EMAIL`, `PERSON`, or `AMOUNT`

Example:

```json
{
  "total_mappings": 4,
  "categories": {
    "AMOUNT": 1,
    "EMAIL": 2,
    "PERSON": 1
  }
}
```

#### `vault_stats` caveats

- This output is safe to expose. It does not include original values.
- The keys in `categories` are token prefixes like `EMAIL` and `PERSON`, not enum-style names like `Email` and `Person`.
- In the current MCP implementation, the stats reflect the loaded in-memory vault state during this server process.
- If stats reset after restart, that is expected today because new MCP-created mappings are not saved back to disk.

### `configure`

Change the detector configuration while the MCP server is running.

Use this when an agent needs to switch industries, temporarily disable a category, or enable a category for a specific task.

#### `configure` request

| Field     | Type                 | Required | Description                |
| --------- | -------------------- | -------- | -------------------------- |
| `profile` | `string \| null`     | No       | Industry profile to load   |
| `enable`  | `string[] \| null`   | No       | Categories to enable       |
| `disable` | `string[] \| null`   | No       | Categories to disable      |

You can send any combination of the three fields.

#### `configure` profiles

Canonical profile names are:

- `general`
- `legal`
- `healthcare`
- `fintech`

Accepted aliases:

- `law` → `legal`
- `health` or `medical` → `healthcare`
- `finance` or `banking` → `fintech`

#### `configure` profile behavior

- `general`: broad default coverage, with phones, IP addresses, internal URLs, and GLiNER-PII NER enabled
- `legal`: adds legal custom patterns such as case numbers and SSNs, preserves common court names, and disables IP and internal URL detection
- `healthcare`: adds healthcare custom patterns such as MRN, NPI, DEA, and ICD codes, preserves terms such as `FDA` and `CDC`, and disables IP and internal URL detection
- `fintech`: adds SWIFT, ISIN, IBAN, and routing-number patterns, enables IP and internal URL detection, and leaves phone numbers off by default

#### `configure` supported toggles

`configure` can enable or disable these categories at runtime:

- `secrets`
- `financial`
- `dates`
- `emails`
- `phone_numbers` or `phone`
- `ip_addresses` or `ip`
- `urls_internal` or `urls`

#### `configure` success response

| Field             | Type               | Description                                         |
| ----------------- | ------------------ | --------------------------------------------------- |
| `active_profile`  | `string \| null`   | Canonical profile name last set through `profile`   |
| `secrets`         | `boolean`          | Whether secret detection is enabled                 |
| `financial`       | `boolean`          | Whether financial detection is enabled              |
| `dates`           | `boolean`          | Whether date detection is enabled                   |
| `emails`          | `boolean`          | Whether email detection is enabled                  |
| `phone_numbers`   | `boolean`          | Whether phone detection is enabled                  |
| `ip_addresses`    | `boolean`          | Whether IP detection is enabled                     |

Example:

```json
{
  "active_profile": "legal",
  "secrets": true,
  "financial": true,
  "dates": true,
  "emails": true,
  "phone_numbers": true,
  "ip_addresses": true
}
```

#### `configure` application order

The server applies configuration in this order:

1. load the requested profile, if any
2. apply `enable`
3. apply `disable`
4. rebuild the detector immediately

That means one call can express both a baseline and an override.

Example: if you want the `legal` profile but still want IP detection on, send:

```json
{
  "profile": "legal",
  "enable": ["ip_addresses"]
}
```

#### `configure` caveats

- Unknown profile names return plain text like `Error: Unknown profile '...'`.
- Unknown category names are silently ignored.
- The detector change affects **future** `detect` and `pseudonymize` calls. It does not rewrite existing tokens or vault contents.
- `general`, `legal`, and `healthcare` profiles enable GLiNER-PII by default. If the sidecar is not reachable, the detector still rebuilds, but the NER layer is skipped and a warning is logged server-side.
- You can toggle `urls_internal`, but the current success response does **not** echo that field back.

### `session_context`

Inspect the session manager used by CloakPipe's session-aware privacy features.

#### `session_context` request

- `session_id` (`string`, required): pass a concrete session ID to inspect it, or the literal string `list` to list all active sessions

#### `session_context` list response

If sessions exist:

```json
{
  "sessions": [
    {
      "session_id": "session-123",
      "message_count": 2,
      "entity_count": 5,
      "coreference_count": 3,
      "sensitivity": "normal",
      "escalation_keywords": [],
      "categories": {
        "Person": 2,
        "Organization": 1,
        "Amount": 2
      },
      "created_at": "2026-05-24T12:00:00+00:00",
      "last_activity": "2026-05-24T12:05:00+00:00"
    }
  ],
  "total": 1
}
```

If no sessions exist, the tool returns:

```json
{
  "sessions": [],
  "note": "No active sessions. Sessions are created when requests include x-session-id header."
}
```

#### `session_context` single-session response

A concrete session lookup returns one safe stats object with these fields:

- `session_id`
- `message_count`
- `entity_count`
- `coreference_count`
- `sensitivity`
- `escalation_keywords`
- `categories`
- `created_at`
- `last_activity`

#### `session_context` missing-session response

If the session does not exist, the tool returns JSON text like:

```json
{
  "error": "Session 'session-123' not found"
}
```

#### `session_context` caveats

- The output is intentionally safe. It returns aggregate stats only, not raw PII and not the full coreference map.
- The current stdio MCP path does **not** create or update sessions during `pseudonymize` calls.
- In other words, `session_context` is exposed by the MCP server, but the rest of the MCP request path does not yet populate it.
- Because of that, `list` will usually be empty unless the MCP server is extended or session state is injected by additional code.

## Practical workflows

### Safe outbound prompt flow

1. Call `pseudonymize` on the raw prompt.
2. Send the returned pseudonymized text to the model.
3. Call `rehydrate` on the model output.

This is the main CloakPipe MCP workflow.

### Detection tuning flow

1. Call `detect` on a representative sample.
2. Call `configure` to switch profile or toggle categories.
3. Call `detect` again and compare the result.
4. Once the configuration looks right, use `pseudonymize` for real traffic.

### Health check flow

1. Call `vault_stats` before and after a few `pseudonymize` calls.
2. Confirm that counts increase during the current process.
3. If you restart the server and counts reset, remember that persistence is currently limited in the MCP path.

## Current implementation limits worth knowing

These are not theoretical edge cases. They follow directly from the current stdio MCP implementation.

1. **Tool results are text-first.** Parse JSON from the returned text instead of assuming a typed MCP object.
2. **New vault mappings are not saved by the MCP server.** They survive for the life of the process, not across restarts.
3. **`session_context` is not wired into `pseudonymize`.** The MCP server exposes session inspection, but the rest of the MCP flow does not create sessions.
4. **`configure` accepts `urls_internal`, but the response does not report its value.**
5. **`detect` returns raw sensitive values.** Use it for review, not for sanitized outbound traffic.

If you keep those five points in mind, the MCP server is straightforward to integrate.
