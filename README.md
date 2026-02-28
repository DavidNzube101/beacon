# Beacon

![Language](https://img.shields.io/badge/built%20with-Rust-orange?style=flat-square&logo=rust)
![Tests](https://img.shields.io/github/actions/workflow/status/DavidNzube101/beacon/release.yml?label=tests&style=flat-square)
![Version](https://img.shields.io/github/v/release/DavidNzube101/beacon?style=flat-square)
![License](https://img.shields.io/badge/license-BUSL--1.1-blue?style=flat-square)

Make any repository agent-ready. Instantly.

Beacon scans a codebase, infers its agent-usable capabilities using AI, and generates a standards-compliant [`AGENTS.md`](
https://github.com/agentmd/agent.md) file making your repo discoverable by any autonomous agent 

---

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/DavidNzube101/beacon/master/install.sh | sh
```

Or with Docker:

```bash
docker run -p 8080:8080 -e GEMINI_API_KEY=your_key ghcr.io/davidnzube101/beacon
```

---

## Quickstart

Set your key, point at a repo, done:

```bash
export GEMINI_API_KEY=your_key_here
beacon generate ./my-project
```

---

## Usage

**Generate an AGENTS.md:**
```bash
beacon generate <path|github-url>

# use a specific provider
beacon generate ./my-project --provider claude --api-key sk-ant-...
beacon generate ./my-project --provider openai --api-key sk-...
beacon generate ./my-project --provider gemini

# custom output path
beacon generate ./my-project --output ./docs/AGENTS.md
```

**Validate an AGENTS.md:**
```bash
beacon validate ./AGENTS.md

# also test if declared endpoints are reachable
beacon validate ./AGENTS.md --check-endpoints
```

**Run as a web API:**
```bash
beacon serve --port 8080
```

---

## Providers

| Provider | `--provider` flag | Key |
|---|---|---|
| Gemini 2.5 Flash | `gemini` (default) | `GEMINI_API_KEY` |
| Claude | `claude` | `CLAUDE_API_KEY` |
| OpenAI GPT-4o | `openai` | `OPENAI_API_KEY` |
| Beacon Cloud | `beacon-ai-cloud` | none — $0.09/run via USDC |

Pass your key with `--api-key` or set the environment variable. For `beacon-ai-cloud` no key is needed — you pay per run in USDC on Base or Solana via x402.

---

## API

```
GET  /health
POST /generate   { "repo_url": "/path/to/repo" }
POST /validate   { "content": "<agents_md_string>" }
```

---

## How it works

1. **Scan**: walks the repo, extracts README, source files, package manifests, and OpenAPI specs
2. **Infer**: sends the context to your chosen AI provider, which identifies capabilities, endpoints, and schemas
3. **Generate**: writes an AAIF-compliant `AGENTS.md` to your repo
