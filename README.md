# Beacon

![Language](https://img.shields.io/badge/built%20with-Rust-orange?style=flat-square&logo=rust)
![Tests](https://img.shields.io/github/actions/workflow/status/DavidNzube101/beacon/release.yml?label=tests&style=flat-square)
![Version](https://img.shields.io/badge/version-0.2.5-blue?style=flat-square)
![License](https://img.shields.io/badge/license-BUSL--1.1-blue?style=flat-square)

**The Verifiable Agentic Protocol.** Make any repository agent-ready. Instantly.

Beacon is a protocol designed for the Web 4.0 agentic economy. It scans your codebase, infers its capabilities using AI, registers a unique on-chain identity, and exposes standardized tools for autonomous agents via the Model Context Protocol (MCP).

---

## Core Features

- **AI-Powered Inference:** Automatically generate AAIF-compliant [AGENTS.md](https://github.com/agentmd/agent.md) manifests.
- **On-Chain Identity:** Register your repository's provenance on Base Mainnet via ERC-7527.
- **Native MCP Server:** Standards-compliant tool discovery for LLMs (Claude, Cursor, etc.).
- **Monetized Validation:** Integrated x402 protocol for pay-per-run verification on Base/Solana.

---

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/DavidNzube101/beacon/master/install.sh | sh
```

---

## Quickstart

**1. Generate Manifest**
```bash
export GEMINI_API_KEY=your_key
beacon generate ./my-project
```

**2. Register On-Chain Identity**
```bash
export AGENT_PRIVATE_KEY=0x...
beacon register ./
```

**3. Serve as MCP Tool**
```bash
beacon serve --port 8080
# AI agents can now connect to http://localhost:8080/sse
```

---

## Usage

### Protocol Commands

| Command | Description |
|---|---|
| `generate` | Scans a repo and creates an AGENTS.md manifest. |
| `register` | Mints an ERC-7527 identity NFT for the repo on Base. |
| `validate` | Checks an AGENTS.md for standards compliance. |
| `serve` | Starts a dual-protocol (REST + MCP) server. |
| `upgrade` | Automatically upgrades Beacon CLI to the latest version. |

### Advanced Registration
Beacon uses a linear bonding curve for registration. Early adopters secure their repository's on-chain provenance at a lower premium.
```bash
beacon register ./ --chain base
```

### AI Providers
| Provider | --provider flag | Key |
|---|---|---|
| Gemini 2.5 Flash | `gemini` (default) | `GEMINI_API_KEY` |
| Claude | `claude` | `CLAUDE_API_KEY` |
| DeepSeek | `deepseek` | `DEEPSEEK_API_KEY` |
| Beacon Cloud | `beacon-ai-cloud` | none — $0.09/run via USDC |

---

## Interoperability (MCP)

Beacon implements the Model Context Protocol. Any agent that speaks MCP can discover Beacon tools automatically.

**Claude Desktop Configuration:**
```json
{
  "mcpServers": {
    "beacon": {
      "url": "https://api.beaconcloud.org/sse"
    }
  }
}
```

---

## How it works

1. **Scan**: Walks the repo, extracting source files, package manifests, and OpenAPI specs.
2. **Infer**: Identifies capabilities, endpoints, and schemas using framework-aware AI inference.
3. **Register**: Wraps the repository URL into a unique on-chain identity token (ERC-7527).
4. **Expose**: Serves tools to the agentic web via standard REST and MCP interfaces.
