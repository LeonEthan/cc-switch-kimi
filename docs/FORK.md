# CC Switch (Kimi Code fork)

> **English** · [中文](#cc-switch-kimi-code-分支中文)

This repository is a **community fork** of the official [farion1231/cc-switch](https://github.com/farion1231/cc-switch) project. It is **not** the official CC Switch product, website, or release channel.

| | Official | This fork (`cc-switch-kimi`) |
|---|----------|-----------------------------|
| Repository | [farion1231/cc-switch](https://github.com/farion1231/cc-switch) | [LeonEthan/cc-switch-kimi](https://github.com/LeonEthan/cc-switch-kimi) |
| Website | [ccswitch.io](https://ccswitch.io) | *(none — use this repo)* |
| Releases / Homebrew / auto-update | Official channels | **Not official.** Prefer building from this source |
| Extra apps | — | **[Kimi Code CLI](https://www.kimi.com/code/)** and **Pi** as first-class managed apps |

Upstream still provides the bulk of the product. This fork adds Kimi Code and Pi management on top of a recent upstream baseline.

---

## What this fork adds (Kimi Code)

Kimi Code is managed like Hermes / OpenCode (additive live config under `~/.kimi-code`), **without** local proxy takeover.

| Area | Behavior |
|------|----------|
| App id | `kimicode` (binary name: `kimi`) |
| Config | `~/.kimi-code/config.toml` (providers), `mcp.json`, `skills/`, sessions |
| Providers | Add / import / switch presets in the Kimi Code tab |
| MCP & Skills | Bidirectional sync when the app is enabled |
| Sessions | Session Manager scans `~/.kimi-code/sessions` (`wire.jsonl`) |
| Usage dashboard | Sync imports turn-level `usage.record` → `kimicode_session` |
| About → CLI lifecycle | Install / update / version (official script + npm fallback; update via `kimi upgrade`) |
| Proxy takeover | **Not implemented** (same class as Hermes/OpenCode) |

---

## What this fork adds (Pi)

[Pi](https://www.npmjs.com/package/@earendil-works/pi-coding-agent) (binary name: `pi`) is managed as an **additive** app like Grok Build — providers coexist in your live Pi config — **with** local proxy takeover, same class as Claude / Codex / Gemini / Grok Build.

| Area | Behavior |
|------|----------|
| App id | `pi` |
| Config | `~/.pi/agent/models.json` (providers), `settings.json` (default selection), sessions |
| Providers | Add / import / switch presets in the Pi tab; switching sets `defaultProvider`/`defaultModel` additively — your other entries, comments, and unknown keys are preserved (JSONC round-trip, never whole-file replacement) |
| Auth | **Pi owns its OAuth.** CC Switch never copies OAuth tokens into its DB/backups/cloud sync and never overwrites or deletes `auth.json` entries with `type: "oauth"` |
| Sessions | Session Manager scans `~/.pi/agent/sessions` (read-only except explicit delete); deletion requires explicit user action |
| Usage dashboard | Sync imports per-request usage (assistant messages, auto-`compaction`, branch `_pi_summary`) into `proxy_request_logs` with `data_source = "pi_session"`; per-entry idempotent request id `pi_session:{session}:{entry_id}`; fingerprint-deduped against proxy rows (no double counting) |
| Proxy takeover | Optional per-app takeover: the selected provider's `baseUrl` is rewritten to the local proxy (`/pi` serves both Anthropic `/v1/messages` and OpenAI `/chat/completions`), with usage logging, health, and failover. Restore returns your config byte-for-byte |
| Compatibility gate | Writes stop with a clear error if the installed Pi is older than the minimum supported version (0.52.7) |
| Skills | **Managed.** CC Switch syncs Skills to `~/.pi/agent/skills/` using Pi's "Agent Skills" conventions (same path shape as Claude/Codex) |
| Prompts | **Managed.** CC Switch writes the global instructions file as `~/.pi/agent/AGENTS.md` (round-trip; preserves existing content) |
| MCP | Not managed — Pi has no native MCP. CC Switch silently skips Pi in MCP sync (same pattern as OpenClaw). Pi's package / theme / extension ecosystem stays Pi-owned |

---

## Correct usage

### Prefer this fork only if you need Kimi Code or Pi in CC Switch

If you do **not** need Kimi Code / Pi management, install the **official** app from [ccswitch.io](https://ccswitch.io) or [farion1231/cc-switch releases](https://github.com/farion1231/cc-switch/releases). You get notarized builds and official auto-update.

### If you use this fork

1. **Install from this repository** (build from source or artifacts published **here**), not from Homebrew `cc-switch` or the official website — those are upstream.
2. **Disable in-app auto-update** (or ignore update prompts). The updater endpoint still points at **official** GitHub releases and can overwrite your fork with a build that has **no Kimi Code**.
3. **Do not mix** official and fork binaries on the same machine without care: both use bundle id `com.ccswitch.desktop` and data at `~/.cc-switch/`.
4. **Backup before first launch** of a fork build that may migrate the DB:
   ```bash
   cp -a ~/.cc-switch ~/.cc-switch.backup-$(date +%Y%m%d)
   ```
5. **Kimi Code CLI** is separate: install via About → Tools, or [Kimi Code docs](https://www.kimi.com/code/). CC Switch only manages providers/MCP/skills/sessions/usage. **Pi** is likewise separate (`npm i -g @earendil-works/pi-coding-agent`); CC Switch only manages providers/sessions/usage/proxy.

### Build & install (macOS example)

```bash
git clone https://github.com/LeonEthan/cc-switch-kimi.git
cd cc-switch-kimi
pnpm install
pnpm tauri build
# App: src-tauri/target/release/bundle/macos/CC\ Switch.app
# Quit official CC Switch first, then replace /Applications if you intend a full swap.
```

Requirements: Node 18+, pnpm, Rust (see [CONTRIBUTING.md](../CONTRIBUTING.md)). Local builds are typically **ad-hoc signed** (not Apple-notarized); first open may need right-click → Open / clear quarantine.

### Data locations (unchanged from official)

- App DB / settings: `~/.cc-switch/`
- Kimi Code live config: `~/.kimi-code/`
- Pi live config: `~/.pi/agent/`

---

## Relationship to upstream

- **Upstream issues / PRs** for general CC Switch features: [farion1231/cc-switch](https://github.com/farion1231/cc-switch).
- **Fork-specific** issues (Kimi Code / Pi integration, fork install docs): [LeonEthan/cc-switch-kimi](https://github.com/LeonEthan/cc-switch-kimi).
- We aim to stay mergeable with upstream; Kimi Code / Pi work is additive where possible.
- Official trademarks, sponsors, and branding remain property of the original project; listed here for product continuity only.

---

## Rollback

1. Quit CC Switch.
2. Restore the previous `.app` (or reinstall official).
3. If the DB schema is newer than official supports (“database too new”), restore a pre-migration backup of `~/.cc-switch/`.

---

# CC Switch (Kimi Code 分支)·中文

本仓库是官方 [farion1231/cc-switch](https://github.com/farion1231/cc-switch) 的**社区分支**，**不是**官方产品、官网或正式发版渠道。

| | 官方 | 本分支 (`cc-switch-kimi`) |
|---|------|---------------------------|
| 仓库 | [farion1231/cc-switch](https://github.com/farion1231/cc-switch) | [LeonEthan/cc-switch-kimi](https://github.com/LeonEthan/cc-switch-kimi) |
| 官网 | [ccswitch.io](https://ccswitch.io) | 无（以本仓库为准） |
| 安装 / 自动更新 | 官网、Homebrew、官方 Releases | **非官方**；请从本仓库源码构建或本仓库产物安装 |
| 增量能力 | — | 将 **Kimi Code CLI** 与 **Pi** 作为一等管理应用 |

### 本分支为 Kimi Code 提供的能力

- 供应商：累加模式写入 `~/.kimi-code/config.toml`
- MCP / Skills / 会话管理
- Usage：从 `wire.jsonl` 的 turn 级 `usage.record` 导入
- About：CLI 安装 / 更新 / 版本探测
- **不做** 本地 Proxy 接管（与 Hermes / OpenCode 同类）

### 本分支为 Pi 提供的能力

- 供应商：累加模式写入 `~/.pi/agent/models.json`，切换只改 `settings.json` 的 `defaultProvider`/`defaultModel`；注释、未知字段与其它供应商条目原样保留（JSONC 往返，绝不整文件替换）
- Auth：**OAuth 始终由 Pi 自己持有**——CC Switch 绝不把 OAuth token 复制进自身数据库/备份/云同步，也绝不覆盖或删除 `auth.json` 中 `type: "oauth"` 的条目
- 会话管理：扫描 `~/.pi/agent/sessions`（只读；删除需用户显式操作）
- Usage：导入逐请求用量——assistant 消息、`compaction`、`branch_summary`，数据源 `pi_session`，幂等键 `pi_session:{session}:{entry_id}`，与代理行指纹去重，绝不双算
- **支持** 本地 Proxy 接管（与 Claude / Codex / Gemini / Grok Build 同类）：接管时仅重写当前供应商条目的 `baseUrl` 指向本地代理（`/pi` 同时服务 Anthropic `/v1/messages` 与 OpenAI `/chat/completions`），含用量日志、健康检查与故障转移；恢复时配置逐字节还原
- 兼容门槛：Pi 版本低于最低支持版本（0.52.7）时停止写入并明确报错
- Skills：**已纳入管理**——按 Pi 的 Agent Skills 约定同步至 `~/.pi/agent/skills/`
- Prompts：**已纳入管理**——全局指令文件落盘为 `~/.pi/agent/AGENTS.md`（往返写入，保留原有内容）
- **不接管** MCP（Pi 无原生 MCP；按 OpenClaw 同款静默跳过模式处理）；Pi 自身的 package / theme / extension 生态仍由 Pi 自治

### 正确使用方式

1. **只需要官方功能** → 请用 [ccswitch.io](https://ccswitch.io) 或官方 Releases。  
2. **需要在 CC Switch 里管理 Kimi Code / Pi** → 使用**本仓库**构建/安装。  
3. **务必关闭应用内自动更新**（或忽略官方更新提示），否则可能被官方包覆盖，丢失 Kimi Code / Pi 能力。  
4. 官方与本分支 **共用** `com.ccswitch.desktop` 与 `~/.cc-switch/`，不要混装两套后却不知道当前跑的是哪一个。  
5. 首次运行分支构建前建议备份：`cp -a ~/.cc-switch ~/.cc-switch.backup-$(date +%Y%m%d)`。

### 本地构建（macOS）

```bash
git clone https://github.com/LeonEthan/cc-switch-kimi.git
cd cc-switch-kimi
pnpm install
pnpm tauri build
```

产物在 `src-tauri/target/release/bundle/macos/CC Switch.app`。本地包多为 ad-hoc 签名，首次打开可能需右键打开。

问题反馈：Kimi Code / Pi 相关请开本仓库 Issue；通用 CC Switch 问题优先走上游。
