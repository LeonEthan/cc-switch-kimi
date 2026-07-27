# CC Switch

CC Switch is a desktop workspace for managing the configuration of AI coding tools.

## Language

**Pi**:
The minimal terminal coding harness documented at `pi.dev`, distributed as `@earendil-works/pi-coding-agent`.
_Avoid_: Pi Agent, pi-ai

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
