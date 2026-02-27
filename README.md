# Beacon

Make any repository agent-ready. Instantly.

Beacon scans a codebase, infers its agent-usable capabilities using Gemini 2.5 Flash, and generates a standards-compliant `AGENTS.md` file. It also validates existing `AGENTS.md` files and ships as a web API.

Built on the [AGENTS.md standard](https://github.com/microsoft/AGENTS.md) governed by the Agentic AI Foundation (Linux Foundation).

---

## Install

**From source:**
```bash
git clone https://github.com/DavidNzube101/beacon
cd beacon
cargo build --release
cp target/release/beacon /usr/local/bin/
```

**Docker:**
```bash
docker build -t beacon .
docker run -p 8080:8080 -e GEMINI_API_KEY=your_key beacon
```

---

## Usage

**Generate an AGENTS.md for a local repo:**
```bash
beacon generate ./my-project
```

**Generate with a custom output path:**
```bash
beacon generate ./my-project --output ./docs/AGENTS.md
```

**Validate an existing AGENTS.md:**
```bash
beacon validate ./AGENTS.md
```

**Validate and check if declared endpoints are reachable:**
```bash
beacon validate ./AGENTS.md --check-endpoints
```

**Start the web API server:**
```bash
beacon serve --port 8080
```

---

## API

**Health check:**
```
GET /health
```

**Generate AGENTS.md from a repo path:**
```
POST /generate
Content-Type: application/json

{ "repo_url": "/path/to/repo" }
```

**Validate an AGENTS.md file:**
```
POST /validate
Content-Type: application/json

{ "content": "<file contents as string>" }
```

---

## Configuration

Beacon requires a Gemini API key for capability inference. Set it as an environment variable or in a `.env` file:

```bash
GEMINI_API_KEY=your_key_here
```

Get a key at [aistudio.google.com](https://aistudio.google.com).

---

## How it works

1. **Scan** — traverses the repo and extracts README, source files, package manifests, and OpenAPI specs
2. **Infer** — sends the extracted context to Gemini 2.5 Flash, which identifies agent-usable capabilities, endpoints, and schemas
3. **Generate** — writes a structured, AAIF-compliant `AGENTS.md` file

---

## Stack

- Rust (axum, tokio, reqwest, clap)
- Gemini 2.5 Flash API
- Docker
- Deployed on Render

---

## License

MIT
