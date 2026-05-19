# pepper — Weekly Orchestrator

## Purpose

CLI binary that runs the full weekly CRM flow by spawning MCP servers over stdio and chaining tool calls: parse VCF → sync DB → get due items → render digest → export `.ics` → send email → build travel snapshot.

## Contents

| Path | Role |
|------|------|
| `src/main.rs` | MCP client orchestration, clap CLI |
| `Cargo.toml` | Depends on `pepper-crm`, `rmcp`, `clap` |

## CLI flags

- `--dry-run` — preview without sending email
- `--recipient` — override digest recipient
- `--force-travel` — rebuild travel snapshot
- `--contacts-dir` — VCF directory override

## Open-source candidate

**Yes.** Thin orchestrator; all logic lives in `pepper-crm` and MCP server crates. Requires locally built MCP server binaries in `./target/debug/`.
