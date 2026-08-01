import { describe, expect, it } from "vitest";
import {
  OMP_DEFAULT_CONFIG,
  OMP_SOURCE_MANAGED,
  OMP_SOURCE_OAUTH,
  isOmpManagedProvider,
  ompProviderPresets,
  ompProviderTypeOptions,
  type OmpProviderType,
} from "@/config/ompProviderPresets";

const VALID_APIS: ReadonlyArray<OmpProviderType> = [
  "openai-completions",
  "openai-responses",
  "openai-codex-responses",
  "azure-openai-responses",
  "anthropic-messages",
  "google-generative-ai",
  "google-vertex",
];

describe("ompProviderPresets", () => {
  it("offers all 7 Omp api protocols in the type dropdown", () => {
    expect(ompProviderTypeOptions.map((o) => o.value)).toEqual(VALID_APIS);
  });

  it("has unique preset names", () => {
    const names = ompProviderPresets.map((p) => p.name);
    expect(new Set(names).size).toBe(names.length);
  });

  it("every preset has required fields and a valid api value", () => {
    for (const preset of ompProviderPresets) {
      expect(preset.name.trim()).not.toBe("");
      expect(VALID_APIS).toContain(preset.settingsConfig.api);
      expect(preset.settingsConfig.models.length).toBeGreaterThan(0);
      expect(preset.settingsConfig.defaultModelId).toBeTruthy();
      expect(
        preset.settingsConfig.models.some(
          (m) => m.id === preset.settingsConfig.defaultModelId,
        ),
      ).toBe(true);
    }
  });

  it("includes the Kimi for Coding preset mirroring the Pi defaults", () => {
    const kimi = ompProviderPresets.find((p) => p.name === "Kimi For Coding");
    expect(kimi).toBeDefined();
    expect(kimi!.category).toBe("cn_official");
    expect(kimi!.icon).toBe("kimi");
    expect(kimi!.settingsConfig.api).toBe("anthropic-messages");
    expect(kimi!.settingsConfig.baseUrl).toBe("https://api.kimi.com/coding/");
    expect(kimi!.settingsConfig.defaultModelId).toBe("k3");
  });

  it("includes a custom OpenAI-compatible template", () => {
    const custom = ompProviderPresets.find((p) => p.isCustomTemplate);
    expect(custom).toBeDefined();
    expect(custom!.category).toBe("custom");
    expect(custom!.settingsConfig.api).toBe("openai-completions");
  });

  it("OMP_DEFAULT_CONFIG is a valid config shape", () => {
    expect(VALID_APIS).toContain(OMP_DEFAULT_CONFIG.api);
    expect(OMP_DEFAULT_CONFIG.models.length).toBeGreaterThan(0);
  });
});

describe("isOmpManagedProvider", () => {
  it("accepts all three spellings of the source marker", () => {
    for (const key of ["_ccSource", "_cc_source", "ccSource"]) {
      expect(isOmpManagedProvider("any", { [key]: OMP_SOURCE_MANAGED })).toBe(
        true,
      );
      expect(isOmpManagedProvider("any", { [key]: OMP_SOURCE_OAUTH })).toBe(
        true,
      );
    }
  });

  it("returns false for user/plain providers and bad input", () => {
    expect(isOmpManagedProvider("any", { _ccSource: "user" })).toBe(false);
    expect(isOmpManagedProvider("any", {})).toBe(false);
    expect(isOmpManagedProvider("any", undefined)).toBe(false);
    expect(isOmpManagedProvider("any", "not-an-object")).toBe(false);
  });
});
