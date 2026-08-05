# Contributing to CC Switch (Kimi Code fork)

> [中文版本](#贡献指南)

This repository is a **community fork** of [farion1231/cc-switch](https://github.com/farion1231/cc-switch) with **Kimi Code** integration. See **[docs/FORK.md](docs/FORK.md)** before contributing.

Thank you for your interest! Please also read the [Code of Conduct](./CODE_OF_CONDUCT.md).

## Where to open issues / PRs

| Topic | Prefer |
|-------|--------|
| Kimi Code integration, fork install/docs, fork-only bugs | **This repo**: [LeonEthan/cc-switch-kimi](https://github.com/LeonEthan/cc-switch-kimi/issues) |
| General CC Switch features, upstream bugs | **Upstream**: [farion1231/cc-switch](https://github.com/farion1231/cc-switch/issues) (link or cherry-pick when relevant) |

## How to Contribute

There are many ways to contribute:

- **Report bugs** — Fork-specific: open an issue on this repository. Upstream regressions: prefer [upstream issues](https://github.com/farion1231/cc-switch/issues).
- **Suggest features** — Kimi Code / fork UX here; general product ideas may belong upstream.
- **Improve docs** — Especially [docs/FORK.md](docs/FORK.md) and README fork notices.
- **Contribute code** — Prefer additive, merge-friendly changes. Open a PR against this fork’s default branch.
- **Translate** — Help improve zh / zh-TW / en / ja for new Kimi Code strings.

> **Security vulnerabilities**: Please do NOT use public issues. See our [Security Policy](./SECURITY.md) instead.

## Development Setup

### Prerequisites

- Node.js 18+ and pnpm 8+
- Rust 1.85+ and Cargo
- [Tauri 2.0 prerequisites](https://v2.tauri.app/start/prerequisites/)

> **pnpm version**: the exact version is pinned via the `packageManager` field
> in `package.json` (and used by CI, which sets pnpm up through Corepack).
> With Corepack enabled locally (`corepack enable`), your pnpm shim switches
> to the pinned version automatically, with a one-time download on first use;
> without Corepack, any pnpm 8+ keeps working (`package-manager-strict-version`
> defaults to off). To upgrade pnpm, edit the `packageManager` field — Dependabot
> bumps no longer cover the version.

### Quick Start

```bash
# Install dependencies
pnpm install

# Start development server with hot reload
pnpm dev
```

### Useful Commands

| Command | Description |
|---------|-------------|
| `pnpm dev` | Start dev server (hot reload) |
| `pnpm build` | Production build |
| `pnpm typecheck` | TypeScript type checking |
| `pnpm test:unit` | Run unit tests |
| `pnpm lint` | ESLint check |
| `pnpm format` | Format code (Prettier) |
| `pnpm format:check` | Check code formatting |

For Rust backend:

```bash
cd src-tauri
cargo fmt        # Format Rust code
cargo clippy     # Run linter
cargo test       # Run tests
```

## Code Style

- **Frontend**: Prettier for formatting, ESLint for linting, strict TypeScript (`pnpm typecheck`)
- **Backend**: `cargo fmt` for formatting, `cargo clippy` for linting
- **Tauri 2.0**: Command names must use camelCase

Run all checks before submitting:

```bash
pnpm typecheck && pnpm format:check && pnpm test:unit
cd src-tauri && cargo fmt --check && cargo clippy && cargo test
```

## Pull Request Guidelines

1. **Open an issue first** for new features — PRs for features that are not a good fit may be closed.
2. **Fork and branch** — Create a feature branch from `main` (e.g., `feat/my-feature` or `fix/issue-123`).
3. **Keep PRs focused** — One feature or fix per PR. Avoid unrelated changes.
4. **Follow the PR template** — Fill in the summary, related issue, and checklist.

### PR Checklist

- [ ] `pnpm typecheck` passes
- [ ] `pnpm format:check` passes
- [ ] `cargo clippy` passes (if Rust code changed)
- [ ] Updated i18n files if user-facing text changed

### Commit Convention

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(provider): add support for new provider
fix(tray): resolve menu not updating after switch
docs(readme): update installation instructions
ci: add format check workflow
chore(deps): update dependencies
```

## AI-Assisted Contributions

We welcome AI-assisted contributions, but **the responsibility stays with you**. AI tools lower the cost of writing code — they do not lower the cost of reviewing it. Maintainers are not obligated to clean up AI-generated output.

By submitting a PR, you agree to the following:

1. **You have read and understood your code.** You must be able to explain any line in your PR. If you cannot, it is not ready for review.
2. **You have tested it yourself.** Every change must be verified locally — not just "it looks right." Do not submit code for platforms or features you cannot test.
3. **PRs must be small and focused.** One issue, one PR. Large, sprawling, multi-topic PRs will be closed.
4. **Open an issue first.** Drive-by PRs with no prior discussion — especially AI-generated ones — may be closed without review.
5. **Maintainers may close without explanation.** PRs that appear to be unreviewed AI output — hallucinated fixes, unnecessary refactors, bulk changes with no context — may be closed at the maintainer's discretion.

**In short**: AI is a tool, not a substitute for understanding. Use it to help you contribute better, not to shift work onto maintainers.

## Internationalization (i18n)

CC Switch supports three languages. When modifying user-facing text:

1. Update **all three** locale files:
   - `src/locales/en/translation.json`
   - `src/locales/zh/translation.json`
   - `src/locales/ja/translation.json`
2. Use the `t()` function from i18next for all UI text.
3. Never hardcode user-facing strings.

## Questions?

- [Open a question](https://github.com/farion1231/cc-switch/issues/new?template=question.yml)
- [GitHub Discussions](https://github.com/farion1231/cc-switch/discussions)

---

# 贡献指南

> [English Version](#contributing-to-cc-switch-kimi-code-fork)

本仓库是 [farion1231/cc-switch](https://github.com/farion1231/cc-switch) 的**社区分支**，增加了 **Kimi Code** 集成。贡献前请阅读 **[docs/FORK.md](docs/FORK.md)** 与[行为准则](./CODE_OF_CONDUCT.md)。

## Issue / PR 去哪开

| 主题 | 优先 |
|------|------|
| Kimi Code 集成、本分支安装/文档、仅本分支的问题 | **本仓库** [LeonEthan/cc-switch-kimi](https://github.com/LeonEthan/cc-switch-kimi/issues) |
| 通用 CC Switch 功能与上游缺陷 | **上游** [farion1231/cc-switch](https://github.com/farion1231/cc-switch/issues) |

## 如何贡献

- **报告 Bug** — 分支相关请在本仓库开 Issue；上游回归优先上游。
- **建议功能** — Kimi Code / 分支体验在此；通用产品建议可走上游。
- **改进文档** — 重点 [docs/FORK.md](docs/FORK.md) 与 README 分支说明。
- **贡献代码** — 尽量 additive、易与上游合并；向本仓库默认分支提 PR。
- **翻译** — 完善 zh / zh-TW / en / ja 中 Kimi Code 相关文案。

> **安全漏洞**：请不要使用公开 Issue 报告。请参阅我们的[安全策略](./SECURITY.md)。

## 开发环境搭建

### 前提条件

- Node.js 18+ 和 pnpm 8+
- Rust 1.85+ 和 Cargo
- [Tauri 2.0 开发环境](https://v2.tauri.app/start/prerequisites/)

> **pnpm 版本**：确切版本由 `package.json` 的 `packageManager` 字段固定
> （CI 通过 Corepack 按此字段安装）。本地启用 Corepack（`corepack enable`）
> 后，pnpm shim 会自动切到该版本（首次使用需一次性下载）；未启用 Corepack
> 时任意 pnpm 8+ 均可正常使用（`package-manager-strict-version` 默认关闭）。
> 升级 pnpm 请直接修改 `packageManager` 字段，不再由 Dependabot 代劳。

### 快速开始

```bash
# 安装依赖
pnpm install

# 启动开发服务器（热重载）
pnpm dev
```

### 常用命令

| 命令 | 说明 |
|------|------|
| `pnpm dev` | 启动开发服务器（热重载） |
| `pnpm build` | 构建生产版本 |
| `pnpm typecheck` | TypeScript 类型检查 |
| `pnpm test:unit` | 运行单元测试 |
| `pnpm lint` | ESLint 检查 |
| `pnpm format` | 格式化代码（Prettier） |
| `pnpm format:check` | 检查代码格式 |

Rust 后端命令：

```bash
cd src-tauri
cargo fmt        # 格式化 Rust 代码
cargo clippy     # 运行 Clippy 检查
cargo test       # 运行测试
```

## 代码规范

- **前端**：使用 Prettier 格式化、ESLint 检查、严格 TypeScript（`pnpm typecheck`）
- **后端**：使用 `cargo fmt` 格式化、`cargo clippy` 检查
- **Tauri 2.0**：命令名必须使用 camelCase

提交前运行所有检查：

```bash
pnpm typecheck && pnpm format:check && pnpm test:unit
cd src-tauri && cargo fmt --check && cargo clippy && cargo test
```

## Pull Request 指南

1. **先开 Issue 讨论** — 新功能请先开 Issue，不适合项目方向的 PR 可能会被关闭。
2. **Fork 并创建分支** — 从 `main` 创建功能分支（如 `feat/my-feature` 或 `fix/issue-123`）。
3. **保持 PR 专注** — 每个 PR 只做一件事，避免无关改动。
4. **遵循 PR 模板** — 填写概述、关联 Issue 和检查清单。

### PR 检查清单

- [ ] `pnpm typecheck` 通过
- [ ] `pnpm format:check` 通过
- [ ] `cargo clippy` 通过（如修改了 Rust 代码）
- [ ] 如修改了用户可见文本，已更新国际化文件

### 提交信息规范

我们使用 [Conventional Commits](https://www.conventionalcommits.org/)：

```
feat(provider): add support for new provider
fix(tray): resolve menu not updating after switch
docs(readme): update installation instructions
ci: add format check workflow
chore(deps): update dependencies
```

## AI 辅助贡献

我们欢迎 AI 辅助的贡献，但**责任始终在你身上**。AI 工具降低了写代码的成本，但并没有降低 review 的成本。维护者没有义务替你清理 AI 的产出。

提交 PR 即表示你同意以下规则：

1. **你已阅读并理解了你的代码。** 你必须能解释 PR 中的每一行。如果做不到，说明还没准备好提交 review。
2. **你已亲自测试过。** 每个改动都必须在本地验证——而不是"看起来对"。不要提交你自己无法测试的平台或功能的代码。
3. **PR 必须小而聚焦。** 一个 Issue 对应一个 PR。大而散、跨多个主题的 PR 会被直接关闭。
4. **先开 Issue 讨论。** 没有事先讨论的"路过式 PR"——尤其是 AI 生成的——可能会被直接关闭。
5. **维护者可以直接关闭。** 看起来是未经审阅的 AI 产出的 PR——虚构的修复、不必要的重构、缺乏上下文的批量改动——维护者可自行决定关闭。

**一句话总结**：AI 是工具，不是理解力的替代品。用它来帮助你更好地贡献，而不是把工作转移给维护者。

## 国际化（i18n）

CC Switch 支持三种语言。修改用户可见文本时：

1. **同时更新三个**语言文件：
   - `src/locales/en/translation.json`
   - `src/locales/zh/translation.json`
   - `src/locales/ja/translation.json`
2. 所有 UI 文本使用 i18next 的 `t()` 函数。
3. 不要硬编码用户可见的字符串。

## 有疑问？

- [提问](https://github.com/farion1231/cc-switch/issues/new?template=question.yml)
- [GitHub 讨论区](https://github.com/farion1231/cc-switch/discussions)
