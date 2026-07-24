/**
 * Shared validation for additive-mode provider keys
 * (OpenCode / OpenClaw / Hermes / Kimi Code).
 */

export const ADDITIVE_PROVIDER_KEY_PATTERN = /^[a-z0-9]+(-[a-z0-9]+)*$/;

export type AdditiveKeyValidation =
  | { ok: true }
  | {
      ok: false;
      reason: "required" | "invalid" | "loading" | "duplicate";
    };

export function validateAdditiveProviderKey(options: {
  key: string;
  isLocked: boolean;
  isLoading: boolean;
  existingKeys: string[];
}): AdditiveKeyValidation {
  const key = options.key.trim();
  if (!key) return { ok: false, reason: "required" };
  if (!ADDITIVE_PROVIDER_KEY_PATTERN.test(key)) {
    return { ok: false, reason: "invalid" };
  }
  if (options.isLoading) return { ok: false, reason: "loading" };
  if (!options.isLocked && options.existingKeys.includes(key)) {
    return { ok: false, reason: "duplicate" };
  }
  return { ok: true };
}

export function normalizeAdditiveProviderKey(raw: string): string {
  return raw.toLowerCase().replace(/[^a-z0-9-]/g, "");
}
