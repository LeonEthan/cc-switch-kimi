# ADR 0001: First-class Pi support

Status: accepted (2026-07-25)

## Context

CC Switch adds Pi (`@earendil-works/pi-coding-agent`, config under `~/.pi/agent`)
as a managed coding tool. Vocabulary and invariants come from `CONTEXT.md`:
journey parity, non-destructive coexistence, Pi-owned OAuth, routing parity,
session continuity, compatibility gate.

## Decisions

1. **Additive mode** (`AppType::Pi`, id `"pi"`). Pi keeps all providers in one
   live file (`models.json` `providers` map) with the selection living in
   `settings.json` (`defaultProvider`/`defaultModel`). This matches the KimiCode
   additive precedent: CC Switch writes every managed provider into the live
   config; "current" == owner of the default selection; provider cards show
   "In use / Enable", never "Remove" for the owner.

2. **File ownership split** (non-destructive coexistence):
   - `~/.pi/agent/models.json` — CC Switch upserts/removes only its managed
     provider entries inside `providers`; unknown keys, comments (JSONC), and
     other providers are preserved (round-trip edit, never whole-file replace).
   - `~/.pi/agent/auth.json` — CC Switch writes API keys as
     `{type:"api_key", key}` per provider id. It never reads into its own DB,
     never overwrites, and never deletes `{type:"oauth"}` entries: Pi-owned
     OAuth stays Pi-owned. A pre-existing OAuth credential for a provider id
     suppresses the API-key write for that id.
   - `~/.pi/agent/settings.json` — CC Switch sets only `defaultProvider` /
     `defaultModel`; all other keys preserved (serde_json Value round-trip).
   - `models-store.json`, `trust.json`, `keybindings.json`, `sessions/` — never
     written by CC Switch (read-only where used).

3. **Pi-owned OAuth**: Pi refreshes its own OAuth tokens. CC Switch does not
   import OAuth credentials, does not register Pi in `commands/auth.rs`
   `ensure_auth_provider`, and does not copy tokens into its DB or sync.

4. **Compatibility gate**: minimum supported Pi version `0.52.7` (models.json
   merge-by-id semantics stable since then; session format v3). Version is
   detected via `pi --version` (cached); below-minimum versions block writes
   with a localized error. Independently, every write validates the on-disk
   schema shape first and refuses on unexpected structure.

5. **Routing parity**: Pi joins the local proxy under its own path namespace
   (`/pi/v1/messages`, `/pi/v1/chat/completions`) — never the unprefixed
   claude/codex routes — covering Pi's `anthropic-messages` and
   `openai-completions` protocols. Requires the proxy_config CHECK-constraint
   table-rebuild migration, a `PiAdapter`, `pi` in `ProxyTakeoverStatus`, and
   the full takeover/restore arm set in `services/proxy.rs`. Same usage
   logging, health check, and failover behavior as peer tools.

6. **Session continuity**: `session_manager/providers/pi.rs` reads Pi session
   JSONL (v3, tree) read-only for search/display; delete only on explicit user
   action. `services/session_usage_pi.rs` imports per-request usage (assistant
   `usage`, plus compaction/branch-summary usage) with deterministic
   `pi_session:{session}:{entry_id}` request ids, data_source `pi_session`,
   registered in the session-sync chain and in `usage_stats` filter lists so
   routed rows are never double-counted.

7. **Management surfaces**: Skills — yes (Pi implements the Agent Skills
   standard; sync to `~/.pi/agent/skills/`). Prompts — yes (global instructions
   file `~/.pi/agent/AGENTS.md`). MCP — no (Pi has no native MCP; mirror the
   OpenClaw silent-ignore pattern). Profiles/workspaces/tray — no (hermes /
   kimicode precedent). Pi-only ecosystem management (packages, extensions,
   themes, keybindings) is out of scope for this release.

8. **Defaults**: visibility default `false` (opt-in, hermes/kimicode precedent);
   CLI tool id `"pi"`, binary `pi`, npm package `@earendil-works/pi-coding-agent`.
