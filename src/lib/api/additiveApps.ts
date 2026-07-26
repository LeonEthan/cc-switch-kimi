// Single source of truth for which apps use the additive provider model
// (providers coexist in the live config; only `defaultProvider` /
// `defaultModel` picks the "current" one). Mirrors the Rust-side
// `AppType::is_additive_mode()` helper in src-tauri/src/app_config.rs.
//
// Apps in this set:
//   - Have no "owner" concept — a provider card doesn't get deleted when it
//     stops being current; switching just rewrites the default selection.
//   - Highlight as "in config" when the provider still appears in the live
//     file, independently of `isCurrent`.
//   - Skip the Claude/Codex/Gemini-style "Remove" main button (their main
//     button is "In use / Enable", per ADR #1 for KimiCode/Pi and the
//     "in-config add/remove" pattern for OpenCode/OpenClaw).
import type { AppId } from "./types";

/**
 * Apps whose live config holds **all** providers additively (vs. switching
 * the live file wholesale to whichever provider is "current"). Includes
 * Hermes / OpenClaw / OpenCode / Kimi Code / Pi per ADR #1 + ADR #7.
 */
export const ADDITIVE_APPS: ReadonlySet<AppId> = new Set<AppId>([
  "opencode",
  "openclaw",
  "hermes",
  "kimicode",
  "pi",
]);

/** Type-guard variant; pass `appId` to one place rather than re-listing. */
export function isAdditiveApp(appId: AppId): boolean {
  return ADDITIVE_APPS.has(appId);
}
