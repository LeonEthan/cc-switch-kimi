# CC Switch

CC Switch is a desktop workspace for managing the configuration of AI coding tools.

## Language

**Pi**:
The minimal terminal coding harness documented at `pi.dev`, distributed as `@earendil-works/pi-coding-agent`.
_Avoid_: Pi Agent, pi-ai

**omp (Oh My Pi)**:
A fork of Pi by can1357, distributed as `@oh-my-pi/pi-coding-agent` (binary `omp`, app id `"omp"`), configured under `~/.omp/agent` (relocatable via `PI_CODING_AGENT_DIR`).
_Avoid_: OhMyPi, omp agent

**Journey parity**:
Pi is managed through the same end-to-end CC Switch workflows as peer coding tools wherever Pi has an equivalent native capability.
_Avoid_: Feature parity, UI cloning

**First-class Pi support**:
Journey parity across CC Switch's existing core management surface, while Pi-only ecosystem management remains outside the first release.
_Avoid_: Pi ecosystem manager

**Non-destructive coexistence**:
CC Switch manages only the Pi configuration it owns while preserving existing Pi settings and unknown extension data so Pi remains independently usable.
_Avoid_: Configuration takeover, whole-file replacement

**Pi-owned OAuth**:
Pi remains the owner of subscription login tokens and refresh behavior; CC Switch may observe their status but does not copy them into its own persistence or sync.
_Avoid_: Imported OAuth, duplicated login tokens

**Routing parity**:
Pi can use CC Switch local routing for supported API protocols and receives the same usage logging, health checking, and failover behavior as peer coding tools.
_Avoid_: Config-only Pi support, silent protocol fallback

**Session continuity**:
CC Switch reads existing Pi sessions for search, display, and usage aggregation without rewriting their native tree or double-counting routed requests.
_Avoid_: Proxy-only history, mutable session import

**Compatibility gate**:
Pi support targets an explicit minimum version, detects required capabilities, and stops unsafe writes when the installed version or schema is incompatible.
_Avoid_: Floating-latest contract, best-effort incompatible writes

**Role-based selection**:
omp has no single "current" provider: `config.yml` `modelRoles` maps the five roles (default/smol/slow/plan/commit) to `<providerKey>/<modelId>` selectors, so a provider may serve multiple roles and several providers are in use simultaneously.
_Avoid_: Single default provider, switchProvider-only switching

**YAML section surgery**:
omp config writes re-serialize only the owning section (`providers:` in `models.yml`, `modelRoles` in `config.yml`); everything outside the section stays byte-identical and whole-file replacement is never used. Managed entries are marked with the `_ccSource: managed` field (never an id prefix). `agent.db` and `models.db` are never written.
_Avoid_: Whole-file YAML replace, `managed:` key prefixes, writes to agent.db/models.db

**omp-owned OAuth**:
omp remains the owner of credentials stored in `agent.db` (OAuth + login-sourced keys); CC Switch queries it read-only only to detect OAuth for import marking and never copies or overwrites it.
_Avoid_: Imported OAuth, duplicated login tokens

**Takeover does not move modelRoles**:
omp proxy takeover rewrites only the managed entry's `baseUrl`/`apiKey` in `models.yml`; role assignments in `config.yml` `modelRoles` are untouched and restore is verbatim.
_Avoid_: Rewriting role selectors, config.yml churn on takeover
