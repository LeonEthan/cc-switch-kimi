# ADR 0002: First-class omp support

Status: accepted (2026-08-01)

## Context

CC Switch adds omp (Oh My Pi, `omp` CLI, npm `@oh-my-pi/pi-coding-agent`,
a fork of Pi by can1357, config under `~/.omp/agent`, relocatable via
`PI_CODING_AGENT_DIR`) as a managed coding tool. Vocabulary and invariants
come from `CONTEXT.md`: journey parity, non-destructive coexistence,
omp-owned OAuth, routing parity, session continuity, compatibility gate.

## Decisions

1. **Additive mode** (`AppType::Omp`, id `"omp"`). All managed providers
   coexist as custom providers in `~/.omp/agent/models.yml` (YAML). Unlike
   every other app, "current" is **role-based**: `config.yml` `modelRoles`
   maps roles (default/smol/slow/plan/commit) to `<providerKey>/<modelId>`
   selectors. A provider may serve multiple roles; multiple providers are
   "in use" simultaneously.

2. **File ownership split** (non-destructive coexistence):
   - `models.yml` — CC Switch upserts/removes only its managed entries under
     `providers:` (marked `_ccSource: managed`). Section-level text surgery:
     only the `providers:` section is re-serialized; everything outside it
     stays byte-identical; user providers and unknown fields are preserved;
     optimistic concurrency (`provider.omp.models.changed_on_disk`). Known
     tradeoff: comments INSIDE the `providers:` section are lost on rewrite
     (the file is tool-managed; outside-section comments are preserved).
   - `config.yml` — only the top-level `modelRoles` key is written, same
     section surgery; theme/webSearchOrder/disabledProviders etc. untouched.
   - `agent.db` (SQLite auth store: OAuth + login-sourced keys) and
     `models.db` (catalog cache) — NEVER written; agent.db is read-only
     queried only to detect OAuth credentials for import read-only marking.
     omp-owned OAuth stays omp-owned.
   - `sessions/` — read-only except explicit delete.

3. **Managed marking via entry field, not id prefix** (YAML constraint):
   Pi used `managed:` id prefixes; YAML keys containing `:` are parse-risky,
   so omp marks entries with `_ccSource: managed` and provider keys are
   validated against `^[a-z0-9][a-z0-9._-]*$` (no `:` or `/`, keeping
   `<key>/<modelId>` selectors parseable).

4. **API keys inline in models.yml** (literal values): omp resolves
   credentials with models.yml `apiKey` at precedence #2, ABOVE stored
   OAuth — so CC Switch writes literal keys there (consistent with other
   apps' live configs; the dir is 0700/0600), and proxy takeover via
   placeholder key works. apiKey is documented by omp as
   "env-var-name-or-literal"; a literal that collides with an env var name
   would be misread — noted, the form hint documents it.

5. **Role-based switching UX**: new `setOmpProviderRole(providerId, role)` /
   `clearOmpModelRole(role)` commands; generic `switchProvider` gained an
   optional `role` (defaults `default` for omp, ignored elsewhere). Provider
   cards show role badges; the main action is a role dropdown
   (assign/unassign per role). Provider deletion strips its roles from
   modelRoles.

6. **Compatibility gate**: minimum supported omp version
   `MIN_OMP_VERSION = "17.0.0"` (models.yml schema + session v3 verified on
   17.2.2). Version is parsed from `omp --version` slash form
   (`omp/17.2.2`); below-minimum blocks writes with a localized error;
   unknown version does not block.

7. **Routing parity**: omp joins the local proxy under `/omp/v1/messages`
   and `/omp/v1/chat/completions` (anthropic-messages + openai-completions
   protocols only), via `OmpAdapter`, `ProxyTakeoverStatus.omp`, and DB
   migration v18→v19 rebuilding the `proxy_config` CHECK constraint (`'omp'`
   added) plus a seeded row. Takeover rewrites the managed entry's
   baseUrl→`{origin}/omp` and apiKey→placeholder and does NOT move
   modelRoles; restore is verbatim.

8. **Session continuity**: session JSONL is Pi's v3 format in a mixed-depth
   layout: main sessions live at `sessions/<cwd>/<timestamp>_<uuid>.jsonl`
   (two levels), subagent sessions one level deeper at
   `sessions/<cwd>/<timestamp>_<uuid>/*.jsonl`; each .jsonl is a session and
   both depths are scanned. Read-only scan/search; resume via
   `omp --resume <id>`. Usage import
   `services/session_usage_omp.rs` (assistant `usage` plus
   compaction/branch-summary as `_omp_summary`, idempotency key
   `omp_session:{session}:{entry}`, data_source `omp_session`, deduped
   against proxy rows).

9. **Management surfaces**: Skills — yes (`<dir>/skills`). Prompts — yes
   (`<dir>/AGENTS.md`). MCP — no (silent-ignore, Pi/OpenClaw precedent).

10. **Defaults**: visibility default `false` (opt-in); CLI tool id `"omp"`,
    binary `omp`, npm package `@oh-my-pi/pi-coding-agent`, FromStr aliases
    `oh-my-pi`/`oh_my_pi`.
