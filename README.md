<div align="center">

# 🔒 CloakPipe

**Privacy proxy for LLM traffic. Detect, mask, and unmask PII in real-time.**

Rust-native · <5ms latency · 33+ entity types · 91.7% real-world protection · OpenAI-compatible · Local-first

[Website](https://cloakpipe.co) · [Docs](https://docs.cloakpipe.co) · [Cloud Dashboard](https://app.cloakpipe.co) · [Discord](https://discord.gg/cloakpipe)

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/cloakpipe.svg)](https://crates.io/crates/cloakpipe)
[![Docker](https://img.shields.io/docker/pulls/cloakpipe/cloakpipe.svg)](https://hub.docker.com/r/cloakpipe/cloakpipe)

</div>

---

## What is CloakPipe?

CloakPipe is a **high-performance privacy layer** for LLM traffic and agent integrations. It detects PII (personally identifiable information), replaces it with safe tokens, forwards sanitized proxy traffic when needed, and restores original values in responses.

Today CloakPipe has three start modes: `server` for direct HTTP API plus MCP stdio tools, `llm-proxy` for clients that point at CloakPipe directly, and `http-proxy` for explicit `HTTP_PROXY`/`HTTPS_PROXY` forward-proxy usage.

**The LLM never sees your real data. Your users see natural responses.**

```
Your App  ──▶  CloakPipe  ──▶  OpenAI-compatible / Anthropic / Supported LLM APIs
                  │
          Detect → Mask → Proxy → Unmask
                  │
           Encrypted Vault
          (AES-256-GCM)
```

---

## Quick Start

### Docker (recommended)

```bash
# Start CloakPipe
docker run -p 3100:3100 ghcr.io/cloakpipe/cloakpipe:latest

# Point your OpenAI SDK at CloakPipe
export OPENAI_BASE_URL=http://localhost:3100/v1

# Done. All LLM calls now go through CloakPipe.
```

### Binary

```bash
# Install (release binary first, cargo fallback)
curl -fsSL https://raw.githubusercontent.com/borgius/cloakpipe/refs/heads/main/install.sh | sh

# Optional direct cargo install from borgius/cloakpipe
cargo install --git https://github.com/borgius/cloakpipe --bin cloakpipe cloakpipe-cli

# Start the direct LLM proxy
cloakpipe start llm-proxy
```

The installer initializes a global CloakPipe home at `~/.cloakpipe` without overwriting existing files. It creates `~/.cloakpipe/cloakpipe.toml`, `~/.cloakpipe/policies/`, and `~/.cloakpipe/models/`. The requested misspellings `~/.cloackpipe` and `cloackpipe.toml` are accepted as compatibility aliases; canonical docs and generated files use `cloakpipe`.

### Choose a start mode

CloakPipe requires an explicit mode when starting.

| Mode | Best for | What the client changes | Upstream auth |
|---|---|---|---|
| `server` | Direct privacy APIs and MCP agent integrations | Call CloakPipe API endpoints directly, or configure an MCP client to run `cloakpipe start server` | No upstream auth for direct API/MCP tools; upstream-backed tree routes still use `proxy.api_key_env` |
| `llm-proxy` | Apps and SDKs that can point at CloakPipe directly | Point the SDK or app at CloakPipe paths such as `http://localhost:<port>/v1` or `/anthropic` | `pass-through` by default; set `auth_mode = "server-key"` to make CloakPipe inject `proxy.api_key_env` |
| `http-proxy` | Apps that support `HTTP_PROXY` or `HTTPS_PROXY` | Keep the SDK/app pointed at the real provider and set proxy environment variables | `pass-through` by default; set `auth_mode = "server-key"` when a bearer-token upstream should come from CloakPipe |

Three important points:

- `server` exposes only the direct CloakPipe HTTP API and MCP stdio tools. It does not proxy LLM chat/embedding requests.
- `llm-proxy` replaces the removed `proxy` mode. If you used the old single-upstream OpenAI-compatible setup, keep the same base-URL override and set `auth_mode = "server-key"`.
- `llm-proxy` is now proxy-only. Direct API endpoints such as `/pseudonymize` and `/v1/detect` live in `server` mode.
- `http-proxy` is the transparent network-proxy mode. Plain HTTP traffic can be inspected and mutated. HTTPS traffic uses CONNECT; by default it is tunneled unchanged, and opt-in HTTPS inspection is available for known or allowlisted LLM hosts after you install a local CloakPipe CA.

#### Mode 1: `server` API and MCP tools

Use `server` when you want CloakPipe's direct privacy API endpoints or MCP tools without proxying LLM provider traffic.

```bash
cloakpipe start server
```

This mode exposes HTTP endpoints such as `/v1/pseudonymize`, `/v1/rehydrate`, `/v1/detect`, `/v1/vault_stats`, `/v1/configure`, `/v1/session_context`, `/tree/...`, and `/sessions/...`. It also starts the stdio MCP server used by agent integrations. It does **not** register the catch-all LLM proxy route.

#### Mode 2: `llm-proxy`

Use `llm-proxy` when your client can point at CloakPipe. It covers the old `proxy` mode and adds multi-provider raw HTTP routing.

Example config:

```toml
[proxy]
listen = "127.0.0.1:8900"
upstream = "https://api.openai.com"
mode = "llm-proxy"
masking_strategy = "similar"
auth_mode = "pass-through"
dry_run = false

[proxy.provider_routes]
anthropic = "https://api.anthropic.com"
```

How to use it:

1. Start CloakPipe with `cloakpipe start llm-proxy`.
2. Point the client at CloakPipe, not at the real provider.
3. For OpenAI-compatible clients, use CloakPipe as the base URL. `llm-proxy` accepts both `/v1/...` and `/chat/...` style paths.
4. For Anthropic clients, use `http://127.0.0.1:8900/anthropic` as the base URL so the SDK's `/v1/messages` request becomes `/anthropic/v1/messages` at CloakPipe.
5. Send the real provider auth header from the client. `auth_mode = "pass-through"` is the default and recommended setting.

To replace the removed `proxy` mode, use the same single upstream and switch auth to server-side injection:

```toml
[proxy]
listen = "127.0.0.1:8900"
upstream = "https://api.openai.com"
api_key_env = "OPENAI_API_KEY"
mode = "llm-proxy"
auth_mode = "server-key"
masking_strategy = "similar"
```

In that setup, CloakPipe injects the provider key from `proxy.api_key_env`. For SDKs that insist on an API key even when talking to CloakPipe, any non-empty placeholder usually works.

What `llm-proxy` does **not** do yet:

- It does not work as a transparent network proxy via `HTTP_PROXY` or `HTTPS_PROXY`.
- It does not accept arbitrary provider prefixes today. The built-in routing currently covers OpenAI-compatible paths and Anthropic-prefixed traffic.

#### Mode 3: `http-proxy` (transparent forward proxy)

Use `http-proxy` when you do **not** want to change application code or SDK base URLs. Your app stays pointed at the real provider, and you configure standard proxy environment variables instead.

Example config:

```toml
[proxy]
listen = "127.0.0.1:8900"
upstream = "https://api.openai.com"
mode = "http-proxy"
masking_strategy = "similar"
auth_mode = "pass-through"

[proxy.http_proxy]
inspect_https = false
tunnel_unknown_hosts = true
allowed_hosts = ["api.openai.com", "api.anthropic.com"]
# Optional corporate proxy chain for CloakPipe egress:
# forward_proxy = "http://corp-proxy.example.com:8080"
# forward_no_proxy = ["localhost", "127.0.0.1"]
```

How to use it:

1. Start CloakPipe with `cloakpipe start http-proxy`.
2. Leave your SDK or app configured for the real provider URL, such as `https://api.openai.com/v1`.
3. Set proxy environment variables for the app process:

```bash
export HTTP_PROXY=http://127.0.0.1:8900
export HTTPS_PROXY=http://127.0.0.1:8900
export NO_PROXY=localhost,127.0.0.1
```

Current behavior:

- Plain `http://...` requests are parsed as forward-proxy absolute-form requests, masked before upstream, and rehydrated on the way back.
- `https://...` requests arrive as CONNECT tunnels. With `inspect_https = false`, CloakPipe opens the tunnel and relays encrypted bytes unchanged.
- With `inspect_https = true`, CloakPipe decrypts, masks, forwards, rehydrates, and re-encrypts HTTPS requests only for built-in known LLM hosts or hosts listed in `allowed_hosts`.
- If `allowed_hosts` is set, plaintext HTTP mutation only runs for matching hosts. Other hosts are forwarded unchanged. CONNECT requests to unknown hosts are tunneled when `tunnel_unknown_hosts = true`.
- `forward_proxy` chains CloakPipe's outbound traffic through a corporate HTTP proxy: `app -> CloakPipe -> corp-proxy -> provider`. CloakPipe does not inherit process `HTTP_PROXY`/`HTTPS_PROXY` for egress, which avoids accidental loops.

##### HTTPS inspection setup

HTTPS inspection is explicit because the client must trust a local CloakPipe root CA.

```bash
cloakpipe http-proxy ca init
cloakpipe http-proxy ca install
# Optional best-effort automatic trust on supported platforms:
cloakpipe http-proxy ca trust --yes
```

Then enable inspection:

```toml
[proxy]
mode = "http-proxy"

[proxy.http_proxy]
inspect_https = true
allowed_hosts = ["api.openai.com", "api.anthropic.com"]
tunnel_unknown_hosts = true
```

Keep your SDK pointed at the real provider, start CloakPipe, and set the app process proxy variables:

```bash
export HTTP_PROXY=http://127.0.0.1:8900
export HTTPS_PROXY=http://127.0.0.1:8900
export NO_PROXY=localhost,127.0.0.1
```

If a runtime does not use the OS trust store, point it at the CA printed by `cloakpipe http-proxy ca print-path`. Common variables are `NODE_EXTRA_CA_CERTS`, `REQUESTS_CA_BUNDLE`, `SSL_CERT_FILE`, and `CURL_CA_BUNDLE`.

##### Corporate proxy chaining

For environments that require outbound traffic through a corporate proxy, configure CloakPipe egress explicitly:

```toml
[proxy.http_proxy]
forward_proxy = "http://corp-proxy.example.com:8080"
forward_no_proxy = ["localhost", "127.0.0.1"]
```

Credentials can be embedded in the URL (`http://user:pass@corp-proxy.example.com:8080`); CloakPipe redacts them in logs/errors. Inbound app proxy credentials stay separate from outbound corporate proxy credentials.

### Verify it works

```bash
curl http://localhost:3100/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -d '{
    "model": "gpt-4",
    "messages": [
      {"role": "user", "content": "Summarize the case for Rajesh Singh, Aadhaar 2345 6789 0123, treated at Apollo Hospital Mumbai."}
    ]
  }'

# CloakPipe logs:
# ✓ Detected 3 entities: PERSON, AADHAAR, ORGANIZATION
# ✓ Masked: Rajesh Singh → PERSON_042, 2345 6789 0123 → AADHAAR_017, Apollo Hospital Mumbai → ORG_003
# ✓ Proxied to api.openai.com (sanitized)
# ✓ Unmasked response: PERSON_042 → Rajesh Singh (restored)
```

---

## Before & After

### What your app sends:

> Summarize the medical history of **Dr. Rajesh Singh** (Aadhaar: **2345 6789 0123**), treated at **Apollo Hospital Mumbai** for cardiac issues since **March 2024**.

### What the LLM sees:

> Summarize the medical history of **PERSON_042** (Aadhaar: **AADHAAR_017**), treated at **ORG_003** for cardiac issues since **DATE_012**.

### What your user gets back:

> **Dr. Rajesh Singh** has been under cardiac care at **Apollo Hospital Mumbai** since **March 2024**. The treatment history includes...

The LLM generates a coherent response using the tokens. CloakPipe restores the original values before returning to your app. The model never saw the real data.

---

## Why CloakPipe?

| | CloakPipe | Presidio | Protecto | LLMGuard |
|---|---|---|---|---|
| **Language** | Rust | Python | Python | Python |
| **Latency** | <5ms | 50–200ms | 50–200ms | 50–200ms |
| **Mode** | Drop-in proxy | Library | Cloud SaaS | Library |
| **Reversible masking** | ✅ Encrypted vault | ❌ Permanent redaction | ✅ Cloud vault | ❌ Permanent |
| **India PII** | ✅ Aadhaar, PAN, UPI, **GSTIN** | ❌ | Aadhaar, PAN only | ❌ |
| **DPDP 2023** | ✅ Built-in policy | ❌ | Claimed | ❌ |
| **Self-hosted** | ✅ Single binary | ✅ | Enterprise only | ✅ |
| **MCP support** | ✅ (via Cloud) | ❌ | ❌ | ❌ |
| **Open source** | ✅ MIT | ✅ MIT | ❌ Closed | ✅ MIT |
| **Price** | Free (open source) | Free | $250–$750/mo | Free |
| **Dependencies** | 0 (single binary) | Python + spaCy | Python + cloud | Python + PyTorch |

---

## How It Works

### Detection Pipeline

CloakPipe uses a multi-layer detection pipeline. Each layer catches what the others miss — the union of all layers achieves 91.7% PII protection on real-world cross-domain data (Slack threads, medical notes, legal memos, financial documents).

```
Input Text
    │
    ▼
┌──────────────────────────────────────────┐
│  Layer 1: Regex + Checksums              │  <1ms
│  Email, phone, SSN, Aadhaar, PAN,       │
│  API keys, IPs, URLs, employee IDs,     │
│  insurance policy numbers, license #s    │
├──────────────────────────────────────────┤
│  Layer 2: Financial Intelligence         │  <1ms
│  Currency amounts ($, EUR, INR, etc.),   │
│  percentages, fiscal dates, periods      │
├──────────────────────────────────────────┤
│  Layer 3: ONNX NER Model                │  5-15ms
│  DistilBERT-PII (63MB, runs on any CPU) │
│  33 entity types: names, addresses,     │
│  orgs, DOB, account numbers, PINs       │
│  No GPU required. No Python dependency.  │
├──────────────────────────────────────────┤
│  Layer 4: Fuzzy Entity Resolution        │  <1ms
│  Jaro-Winkler similarity matching       │
│  Links "Dr. R. Singh" and              │
│  "Rajesh Singh" as same entity           │
├──────────────────────────────────────────┤
│  Layer 5: Custom TOML Rules              │  <1ms
│  User-defined patterns for               │
│  domain-specific identifiers             │
└──────────────────────────────────────────┘
    │
    ▼
Masked Output (total: <20ms on any laptop CPU)
```

#### NER Backend Options

| Backend | Config | Size | Speed | Hardware | Use Case |
|---|---|---|---|---|---|
| **DistilBERT-PII** | `distilbert_pii` | 63MB | 5-15ms | Any CPU | Default. 33 entity types, runs everywhere |
| **GLiNER-PII sidecar** | `gliner_pii` | 2.3GB | 300ms | 4GB+ RAM | Zero-shot custom entity types via Python sidecar |
| BERT NER | `bert` | ~400MB | 20-40ms | Any CPU | Legacy 4-type NER (PER/ORG/LOC/MISC) |
| GLiNER2 | `gliner` | ~800MB | 50ms | Any CPU | Legacy zero-shot NER |

Downloadable/default model assets live under `~/.cloakpipe/models/` by default. Use `cloakpipe ner download` for DistilBERT-PII, or `cloakpipe ner download --model gliner_pii` to bootstrap the managed GLiNER-PII sidecar runtime at `~/.cloakpipe/gliner-pii-venv/`. Run `cloakpipe ner status` to see all supported NER backends and their current status.

### Similar-value pseudonymization

By default, sensitive values are replaced with **similar fake values** — emails stay email-shaped, phone numbers stay phone-shaped, and secrets keep recognizable prefixes without exposing the original value.

Mappings are **deterministic within a session** — the same entity always maps to the same fake value. This means the LLM maintains coherence across the conversation.

Mappings are **non-deterministic across sessions** — the same entity maps to a different fake value in a new session, preventing cross-session correlation.

### Encrypted Vault

All entity ↔ token mappings are stored in a local vault encrypted with AES-256-GCM. The vault never leaves your infrastructure. There is no cloud dependency.

---

## Supported Entity Types

### Standard PII

| Entity | Example | Detection |
|---|---|---|
| Person Name | John Smith, Dr. Priya Sharma | NER |
| Email Address | user@example.com | Regex |
| Phone Number | +1-555-0123, +91 98765 43210 | Regex |
| Credit Card | 4532-1234-5678-9012 | Regex + Luhn |
| SSN | 123-45-6789 | Regex |
| Date of Birth | 15/03/1990, March 15, 1990 | NER |
| Address | 123 MG Road, Pune 411001 | NER |
| IP Address | 192.168.1.1, 2001:db8::1 | Regex |
| Organization | Apollo Hospital, HDFC Bank | NER |
| Medical Term | diabetes, cardiac arrest | NER |
| Bank Account | IFSC + account number | Regex |
| Passport Number | J1234567 | Regex |
| License Plate | MH 12 AB 1234 | Regex |
| URL | https://internal.company.com | Regex |
| API Key | sk-live_xxx, AKIA... | Regex |

### India-Specific PII 🇮🇳

| Entity | Format | Example |
|---|---|---|
| **Aadhaar Number** | 12 digits (XXXX XXXX XXXX) | 2345 6789 0123 |
| **PAN Card** | ABCDE1234F | BNZPM2501F |
| **UPI ID** | name@bank | rajesh@okicici |
| **Indian Phone** | +91 XXXXX XXXXX | +91 98765 43210 |
| **GSTIN** | 15-char alphanumeric | 27AAPFU0939F1ZV |
| **Indian Passport** | Letter + 7 digits | J1234567 |

No other open-source LLM privacy tool handles Indian PII natively.

---

## Integration Examples

Use the mode table above to choose the right base URL. The examples below call out which mode they assume.

### OpenAI Python SDK

```python
from openai import OpenAI

# Just change the base URL. That's it.
client = OpenAI(
    base_url="http://localhost:3100/v1",  # llm-proxy mode for OpenAI-compatible paths
    api_key="sk-your-openai-key"          # Your real API key
)

response = client.chat.completions.create(
    model="gpt-4",
    messages=[
        {"role": "user", "content": "Analyze the account for Priya Sharma, PAN BNZPM2501F"}
    ]
)

# CloakPipe detected PAN and person name, masked them,
# sent sanitized prompt to OpenAI, and unmasked the response.
print(response.choices[0].message.content)
```

### LangChain

```python
from langchain_openai import ChatOpenAI

llm = ChatOpenAI(
    model="gpt-4",
    openai_api_base="http://localhost:3100/v1",  # llm-proxy mode for OpenAI-compatible paths
    openai_api_key="sk-your-key"
)

response = llm.invoke("Summarize patient records for Aadhaar 2345 6789 0123")
```

### Anthropic SDK

This example requires `proxy.mode = "llm-proxy"`.

```python
from anthropic import Anthropic

client = Anthropic(
    base_url="http://localhost:3100/anthropic",  # llm-proxy mode on CloakPipe
    api_key="sk-ant-your-key"
)

message = client.messages.create(
    model="claude-sonnet-4-20250514",
    max_tokens=1024,
    messages=[
        {"role": "user", "content": "Review the loan application for Amit Patel, PAN ABCDE1234F"}
    ]
)
```

### curl

```bash
# OpenAI-compatible request through CloakPipe
curl http://localhost:3100/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -d '{
    "model": "gpt-4",
    "messages": [{"role": "user", "content": "Your prompt with PII here"}]
  }'
```

### Vercel AI SDK

```typescript
import { openai } from '@ai-sdk/openai';
import { generateText } from 'ai';

const result = await generateText({
  model: openai('gpt-4', {
    baseURL: 'http://localhost:3100/v1',  // llm-proxy mode for OpenAI-compatible paths
  }),
  prompt: 'Analyze the customer data for Rajesh, Aadhaar 2345 6789 0123',
});
```

---

## CLI

```bash
# Scan text for PII (no proxy, just detection)
cloakpipe scan "Dr. Rajesh Singh, Aadhaar 2345 6789 0123"
# Output:
# ✓ PERSON: "Dr. Rajesh Singh" (confidence: 0.97)
# ✓ AADHAAR: "2345 6789 0123" (confidence: 1.00)

# Mask text (replace PII with similar fake values by default)
cloakpipe mask "Contact Priya at priya@example.com or +91 98765 43210"
# Output: "Contact PERSON_001 at chris.hall@gmail.com or +91 464 316 6112"

# NER is enabled by default. Disable it for a scan with --no-ner.
cloakpipe scan assets/example.md --no-ner

# Start the direct LLM proxy server
cloakpipe start llm-proxy

# Start with a specific bundled policy config
cloakpipe --config policies/dpdp.toml start llm-proxy

# Check proxy health
cloakpipe health
```

---

## Self-Hosted Admin UI

CloakPipe ships a self-hosted **admin UI** for local operators: a React + TypeScript
+ Vite SPA (TanStack Router/Query/Table) to manage profiles, policies, detection
categories/rules, audit logs, and the vault. It lives in [`apps/admin-ui`](apps/admin-ui)
and talks to the admin API exposed under `/admin/api/*`.

The admin API is mounted **only** in `server` mode:

```bash
# 1. Start CloakPipe in server mode (exposes /admin/api/*)
cloakpipe start server

# 2. Serve the admin UI (zero-dependency npm package)
npx cloakpipe serve
# → CloakPipe admin UI → http://127.0.0.1:8420
```

`npx cloakpipe serve` serves the compiled SPA and reverse-proxies `/admin`, `/v1`,
`/tree`, `/sessions`, and `/health` to the backend (default `http://127.0.0.1:8400`,
override with `--backend` / `CLOAKPIPE_BASE_URL`), so the UI runs same-origin.

**Pages:** Overview · Profiles · Policies · Categories & Rules · Audit Logs ·
Vault & Secrets · Sessions.

**Environment variables:**

| Variable                  | Used by         | Default               | Purpose                                    |
| ------------------------- | --------------- | --------------------- | ------------------------------------------ |
| `VITE_CLOAKPIPE_BASE_URL` | SPA (dev/build) | same-origin           | Backend base URL for the SPA.              |
| `CLOAKPIPE_BASE_URL`      | `cloakpipe serve` | `http://127.0.0.1:8400` | Backend to reverse-proxy.                |
| `PORT` / `HOST`           | `cloakpipe serve` | `8420` / `127.0.0.1`  | Where the admin UI server listens.         |

**Limitations & security:**

- ⚠️ **No built-in authentication.** The admin API can read/modify policies and
  reveal vault secrets. It is intended for **trusted/local** deployments — bind to
  `127.0.0.1` (default) or front it with an authenticating reverse proxy. Do **not**
  expose `/admin/api/*` to untrusted networks.
- Vault originals are **redacted by default**; revealing them requires explicit
  confirmation and writes an audit event.
- Audit querying is fully supported on the **SQLite** backend; the **JSONL** backend
  is best-effort and a **disabled** backend returns a clear unsupported state.
- Listener/upstream/masking changes from a policy require a **restart**; detection
  settings apply live.
- This UI targets **self-hosted/local** CloakPipe and is independent of CloakPipe
  Cloud.

See [`apps/admin-ui/README.md`](apps/admin-ui/README.md) and
[`packages/cloakpipe-serve/README.md`](packages/cloakpipe-serve/README.md) for details.

---

## Configuration

### Environment Variables

```bash
# Proxy settings
CLOAKPIPE_PORT=3100                    # Proxy port (default: 3100)
CLOAKPIPE_HOST=0.0.0.0                # Bind address (default: 0.0.0.0)
CLOAKPIPE_LOG_LEVEL=info               # Log level: debug, info, warn, error

# LLM provider
CLOAKPIPE_UPSTREAM_URL=https://api.openai.com  # Default upstream LLM API
CLOAKPIPE_TIMEOUT=30                   # Request timeout in seconds

# Detection
# Select a bundled policy with: cloakpipe --config policies/dpdp.toml start llm-proxy
CLOAKPIPE_MIN_CONFIDENCE=0.8          # Minimum NER confidence threshold (0.0–1.0)

# Global CloakPipe home
CLOAKPIPE_HOME=~/.cloakpipe           # Global config, policies, models, and managed runtimes
CLOAKPIPE_CONFIG_HOME=~/.cloakpipe    # Backward-compatible alias for the same role
CLOAKPIPE_MODEL_DIR=~/.cloakpipe/models/distilbert-pii # Optional model download target override

# Vault
CLOAKPIPE_VAULT_PATH=./vault.db       # Encrypted vault file path
CLOAKPIPE_VAULT_KEY=                   # 256-bit encryption key (auto-generated if empty)

# Cloud (optional, for dashboard users)
CLOAKPIPE_CLOUD_TOKEN=                 # Cloud dashboard token (app.cloakpipe.co)
```

### Policy Files

If you omit `--config`, CloakPipe searches from the current directory upward for `cloakpipe.toml`, then `cloackpipe.toml`, and finally falls back to the global config at `~/.cloakpipe/cloakpipe.toml`. Relative paths inside the selected config, such as vault, audit, tree storage, and model paths, are resolved relative to that config file.

CloakPipe ships framework-specific policy presets as full `cloakpipe.toml`-compatible files in [`policies/`](policies/). Use them with the global `--config` flag:

```bash
cloakpipe --config policies/dpdp.toml start llm-proxy
```

Create or edit the active policy interactively with:

```bash
cloakpipe policy edit
cloakpipe --config dpdp.toml policy edit
```

The editor can create a missing local policy from defaults, edit an existing `--config` file, or edit the installed user copy of a bundled preset name such as `dpdp.toml`. It supports built-in detection toggles, the default replacement strategy, NER settings, custom regex patterns, the preserve list, and the force list. Replacement strategy is policy-level; per-category replacement strategies are not currently part of the config schema.

Example (`policies/dpdp.toml`):

```toml
[detection]
secrets = true
financial = false
dates = true
emails = true
phone_numbers = true
ip_addresses = true
urls_internal = false

[detection.custom]
patterns = [
  { name = "upi_id", regex = "\\b[A-Za-z0-9._-]{2,}@[A-Za-z][A-Za-z0-9._-]{1,63}\\b", category = "UPI_ID" },
  { name = "gstin", regex = "\\b\\d{2}[A-Z]{5}\\d{4}[A-Z][1-9A-Z]Z[0-9A-Z]\\b", category = "GSTIN" },
  { name = "bank_account_in", regex = "(?i)\\b(?:account|a/c)\\s*(?:number|no\\.?|#)?[:\\s-]*\\d{9,18}\\b", category = "BANK_ACCOUNT" },
]
```

Pre-built policies included: `dpdp.toml`, `gdpr.toml`, `hipaa.toml`, `pci-dss.toml`, `minimal.toml`

See [`policies/README.md`](policies/README.md) for the framework mapping, included patterns, and compliance caveats.

---

## Architecture

<div align="center">
<img src="cloakpipe_architecture_overview.svg" alt="CloakPipe Architecture Overview" width="680" />
</div>

CloakPipe is built as a modular Rust workspace with 8 crates:

```
cloakpipe/
├── crates/
│   ├── cloakpipe-core       # Detection, replacement, vault, rehydration
│   ├── cloakpipe-proxy      # HTTP proxy server (axum, OpenAI-compatible)
│   ├── cloakpipe-tree       # CloakTree: vectorless LLM-driven retrieval
│   ├── cloakpipe-vector     # ADCPE distance-preserving vector encryption
│   ├── cloakpipe-local      # Fully local mode (candle-rs embeddings + LanceDB)
│   ├── cloakpipe-audit      # Compliance logging and audit trails
│   ├── cloakpipe-mcp        # MCP server (6 tools via rmcp)
│   └── cloakpipe-cli        # CLI interface (scan, mask, serve, vault, session)
├── policies/
│   ├── default.toml
│   ├── dpdp.toml
│   ├── gdpr.toml
│   ├── hipaa.toml
│   ├── pci-dss.toml
│   └── minimal.toml
├── Cargo.toml
├── LICENSE
└── README.md
```

### Crate Dependency Graph

```
cloakpipe-cli
    ├── cloakpipe-proxy
    │       ├── cloakpipe-core
    │       ├── cloakpipe-tree
    │       ├── cloakpipe-vector
    │       └── cloakpipe-audit
    └── cloakpipe-mcp
            └── cloakpipe-core
```

Each crate is independently usable. If you only need PII detection in your Rust app without the proxy, depend on `cloakpipe-core` directly.

---

## Benchmarks

### Real-World E2E Protection Test

Tested on 4 cross-domain scenarios (Slack threads, invoice emails, medical notes, legal documents) — messy, unpredictable text that real users paste into LLMs. Not crafted for any detection system.

| Metric | CloakPipe (v0.9) | Regex Only | nvidia/gliner-PII |
|---|---|---|---|
| **PII protection rate** | **91.7%** (55/60) | 53.4% | 65.9% |
| **Names detected** | ✅ | ❌ | ✅ |
| **Addresses detected** | ✅ | ❌ | ✅ |
| **Financial amounts** | ✅ | ✅ | ❌ |
| **API keys / secrets** | ✅ | ✅ | ❌ |
| **Custom IDs (EMP-, INS-)** | ✅ | ❌ | ❌ |
| **Model size** | **63MB** | 0 | 2.3GB |
| **Latency per request** | **5-20ms** | <1ms | 300ms |
| **Requires GPU** | **No** | No | No (slow) |
| **Requires Python** | **No** | No | Yes |

### Per-Scenario Results

| Scenario | Items | Protected | Leaked to LLM |
|---|---|---|---|
| Slack thread (VC deal) | 15 | 87% | 2 items |
| Invoice email (financial) | 15 | 93% | 1 item |
| Doctor's notes (medical) | 14 | 86% | 2 items |
| Immigration case (legal) | 16 | **100%** | 0 items |

### Response Quality

Both protected and unprotected LLM calls produce coherent, usable responses. The LLM sees similar fake values instead of raw sensitive data, preserving value structure for better reasoning. Rehydration restores all original data with perfect roundtrip fidelity.

### Latency

| Tool | Language | Avg Latency | P99 Latency | Accuracy (F1) | Reversible |
|---|---|---|---|---|---|
| **CloakPipe OSS** | Rust | **3.2ms** | **4.8ms** | **0.94** | ✅ |
| **CloakPipe Cloud** | Rust + GLiNER2 | **4.1ms** | **6.2ms** | **0.99** | ✅ |
| Presidio | Python | 87ms | 142ms | 0.84 | ❌ |
| LLMGuard | Python | 112ms | 198ms | 0.82 | ❌ |
| Regex-only | Any | 0.5ms | 0.8ms | 0.61 | ❌ |

---

## Cloud Dashboard

Need analytics, audit trails, or team features? **[CloakPipe Cloud](https://app.cloakpipe.co)** adds a dashboard on top of the open-source proxy.

**The proxy always runs on your infra. PII never leaves your network.** Only anonymized telemetry (entity counts, latency metrics) goes to the dashboard.

| Feature | OSS (Free) | Cloud Pro ($99/mo) | Cloud Business ($499/mo) |
|---|---|---|---|
| Core proxy + detection | ✅ | ✅ | ✅ |
| Encrypted vault | ✅ | ✅ | ✅ |
| Policy templates | ✅ | ✅ | ✅ |
| India PII (Aadhaar, PAN, UPI) | ✅ | ✅ | ✅ |
| Dashboard + analytics | — | ✅ | ✅ |
| Audit trail export | — | ✅ | ✅ |
| Compliance reports | — | ✅ | ✅ |
| Privacy Chat UI | — | ✅ | ✅ |
| Multi-user | — | Up to 10 | Unlimited |
| RBAC + SSO | — | — | ✅ |
| Custom entity types | — | — | ✅ |
| Webhook alerts | — | — | ✅ |
| Kubernetes Helm chart | — | — | ✅ |
| MCP Server (6 tools) | — | — | ✅ |
| Support | Community | Email | Priority |

→ [app.cloakpipe.co](https://app.cloakpipe.co)

---

## Compliance

CloakPipe helps you meet regulatory requirements by ensuring PII never reaches a third-party model. We only claim what we can prove — no vendor-badge theatre.

| Framework | What CloakPipe provides | Can we claim it? |
|---|---|---|
| **DPDP Act 2023** (India) | Detects Aadhaar, PAN, UPI, GSTIN, and contextual Indian bank-account references. Self-hosted mode keeps data within your infrastructure — no cross-border transfer of personal data. Pre-built `policies/dpdp.toml` profile. | ✅ "Supports DPDP compliance" — no certification body exists; compliance is technical. |
| **GDPR** (EU) | Pseudonymization is explicitly recognized under GDPR Art. 25 (data protection by design). Tokens replace personal data before it reaches any third-party processor. | ✅ "GDPR-ready" — self-attested or validated by legal counsel. |
| **HIPAA** (US) | PHI detection (patient IDs, diagnoses, medications), AES-256-GCM encrypted vault, tamper-evident audit logs meet HIPAA Security Rule technical safeguards. | ✅ "Supports HIPAA workflows" — HIPAA has no official certification body. |
| **PCI-DSS** | Credit-card PAN, expiry, CVV, and track-data detection defaults with encrypted vault and no plaintext storage. Pre-built `policies/pci-dss.toml`. | ✅ "Supports PCI-DSS workflows" — formal QSA audit required for full certification. |
| **SOC 2 Type II** | Structured audit logging, access controls, and incident response processes in place. Formal audit in roadmap. | 🔜 In progress — will not claim until third-party audit is complete. |

Pre-built policy files are included in [`policies/`](policies/):

```
policies/
├── default.toml   # Baseline CloakPipe config
├── dpdp.toml      # India Digital Personal Data Protection Act 2023
├── gdpr.toml      # EU General Data Protection Regulation
├── hipaa.toml     # US Health Insurance Portability and Accountability Act
├── pci-dss.toml   # Payment Card Industry Data Security Standard
└── minimal.toml   # Minimal — only high-confidence structured PII
```

---

## Deployment

### Docker Compose

```yaml
version: '3.8'
services:
  cloakpipe:
    image: ghcr.io/cloakpipe/cloakpipe:latest
    command: ["--config", "/etc/cloakpipe/policies/dpdp.toml", "start"]
    ports:
      - "3100:3100"
    environment:
      - CLOAKPIPE_UPSTREAM_URL=https://api.openai.com
      - CLOAKPIPE_LOG_LEVEL=info
    volumes:
      - cloakpipe-vault:/data/vault
      - ./policies:/etc/cloakpipe/policies:ro
    restart: unless-stopped

volumes:
  cloakpipe-vault:
```

### Systemd

```ini
[Unit]
Description=CloakPipe LLM Privacy Proxy
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/cloakpipe serve --port 3100
Restart=always
Environment=CLOAKPIPE_UPSTREAM_URL=https://api.openai.com

[Install]
WantedBy=multi-user.target
```

---

## Contributing

We welcome contributions. See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

**Good first issues:**
- Add new regex pattern for a PII type
- Improve NER accuracy on Indian names
- Add integration example (Haystack, LlamaIndex, etc.)
- Write documentation for a use case

**Development setup:**

```bash
git clone https://github.com/borgius/cloakpipe.git
cd cloakpipe
cargo build
cargo test
cargo run -p cloakpipe-cli -- serve --port 3100
```

---

## Roadmap

- [x] Core proxy with PII detection and masking
- [x] AES-256-GCM encrypted vault
- [x] Regex + ONNX NER detection pipeline
- [x] Jaro-Winkler fuzzy entity resolution
- [x] India PII support (Aadhaar, PAN, UPI, GSTIN)
- [x] CloakTree: vectorless LLM-driven retrieval
- [x] ADCPE distance-preserving vector encryption
- [x] Industry profiles (legal, healthcare, fintech)
- [x] MCP server (6 tools)
- [x] Session-aware pseudonymization + coreference resolution
- [x] DistilBERT-PII NER (63MB ONNX, 33 entity types, runs on any CPU)
- [x] nvidia/gliner-PII sidecar backend (zero-shot custom entities)
- [x] Real-world E2E benchmarks (91.7% protection on cross-domain data)
- [ ] Anthropic API native format support
- [ ] Multi-language NER (Hindi, Marathi, Tamil)
- [ ] WebSocket proxy mode
- [ ] Custom entity type plugins (WASM)
- [ ] TEE support (AWS Nitro Enclaves)

---

## Security

CloakPipe is security-focused software. If you find a vulnerability, please report it responsibly:

**Email:** security@cloakpipe.co

Do **not** file a public GitHub issue for security vulnerabilities.

---

## License

Apache-2.0. See [LICENSE](LICENSE).

The CloakPipe Cloud dashboard and enterprise features are proprietary (BUSL-1.1).

---

<div align="center">

**Built in Rust. Made in Pune, India.**

[Website](https://cloakpipe.co) · [Docs](https://docs.cloakpipe.co) · [Cloud](https://app.cloakpipe.co) · [Twitter](https://twitter.com/cloakpipe) · [Discord](https://discord.gg/cloakpipe)

</div>
