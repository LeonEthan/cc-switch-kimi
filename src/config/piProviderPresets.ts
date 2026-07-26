/**
 * Pi (pi.dev) provider presets.
 * Live format: ~/.pi/agent/models.json `providers.<key>` (JSONC), selection in
 * settings.json `defaultProvider` / `defaultModel`. The DB fragment uses
 * camelCase and stores the protocol in `type`; Rust maps it to models.json's
 * `api` key on write.
 */
import type { ProviderCategory } from "../types";
import type { PresetTheme, TemplateValueConfig } from "./claudeProviderPresets";

export interface PiModel {
  id: string;
  name?: string;
  /** Per-model API override (defaults to the provider's `type`). */
  api?: string;
  reasoning?: boolean;
  input?: string[];
  contextWindow?: number;
  maxTokens?: number;
}

/** Subset of Pi's KnownApi protocols offered in the form dropdown. */
export type PiProviderType =
  | "anthropic-messages"
  | "openai-completions"
  | "openai-responses"
  | "google-generative-ai";

export interface PiProviderSettingsConfig {
  type: PiProviderType;
  apiKey?: string;
  baseUrl?: string;
  models: PiModel[];
  defaultModelId?: string;
  displayName?: string;
  customHeaders?: Record<string, string>;
  /** models.json `authHeader`: send `Authorization: Bearer <apiKey>`. */
  authHeader?: boolean;
}

export interface PiProviderPreset {
  name: string;
  nameKey?: string;
  websiteUrl: string;
  apiKeyUrl?: string;
  settingsConfig: PiProviderSettingsConfig;
  isOfficial?: boolean;
  isPartner?: boolean;
  primePartner?: boolean;
  partnerPromotionKey?: string;
  category?: ProviderCategory;
  templateValues?: Record<string, TemplateValueConfig>;
  theme?: PresetTheme;
  icon?: string;
  iconColor?: string;
  isCustomTemplate?: boolean;
}

export const piProviderTypeOptions: Array<{
  value: PiProviderType;
  label: string;
}> = [
  { value: "anthropic-messages", label: "Anthropic Messages" },
  { value: "openai-completions", label: "OpenAI Chat Completions" },
  { value: "openai-responses", label: "OpenAI Responses" },
  { value: "google-generative-ai", label: "Google Generative AI" },
];

export const piProviderPresets: PiProviderPreset[] = [
  {
    name: "Kimi For Coding",
    websiteUrl: "https://www.kimi.com/code/?aff=cc-switch",
    apiKeyUrl: "https://www.kimi.com/code/?aff=cc-switch",
    category: "cn_official",
    icon: "kimi",
    settingsConfig: {
      // Anthropic-compatible endpoint; OpenAI-compatible is at /coding/v1
      type: "anthropic-messages",
      baseUrl: "https://api.kimi.com/coding/",
      apiKey: "",
      defaultModelId: "k3",
      models: [
        {
          id: "k3",
          name: "K3",
          reasoning: true,
          contextWindow: 1048576,
          maxTokens: 65536,
        },
        {
          // 256K variant of K3: image input only (no video)
          id: "k3-256k",
          name: "K3 (256K)",
          reasoning: true,
          contextWindow: 262144,
          maxTokens: 32768,
        },
        {
          id: "kimi-for-coding",
          name: "K2.7 Coding",
          reasoning: true,
          contextWindow: 262144,
          maxTokens: 32768,
        },
        {
          id: "kimi-for-coding-highspeed",
          name: "K2.7 Coding Highspeed",
          reasoning: true,
          contextWindow: 262144,
          maxTokens: 32768,
        },
      ],
    },
  },
  {
    name: "Anthropic",
    websiteUrl: "https://console.anthropic.com",
    apiKeyUrl: "https://console.anthropic.com/settings/keys",
    category: "official",
    icon: "anthropic",
    settingsConfig: {
      type: "anthropic-messages",
      baseUrl: "https://api.anthropic.com",
      apiKey: "",
      defaultModelId: "claude-fable-5",
      models: [
        {
          id: "claude-fable-5",
          name: "Claude Fable 5",
          reasoning: true,
          contextWindow: 1000000,
          maxTokens: 128000,
        },
        {
          id: "claude-sonnet-5",
          name: "Claude Sonnet 5",
          reasoning: true,
          contextWindow: 1000000,
          maxTokens: 64000,
        },
        {
          id: "claude-haiku-4-5-20251001",
          name: "Claude Haiku 4.5",
          reasoning: false,
          contextWindow: 200000,
          maxTokens: 64000,
        },
      ],
    },
  },
  {
    name: "OpenAI Compatible",
    websiteUrl: "",
    category: "custom",
    isCustomTemplate: true,
    icon: "openai",
    settingsConfig: {
      type: "openai-completions",
      baseUrl: "https://api.openai.com/v1",
      apiKey: "",
      defaultModelId: "gpt-5.2",
      models: [
        {
          id: "gpt-5.2",
          name: "GPT-5.2",
          reasoning: true,
          contextWindow: 1047576,
          maxTokens: 128000,
        },
      ],
    },
  },
];

/** Marker field written by Rust `get_providers` for managed/OAuth entries. */
export const PI_SOURCE_FIELD = "_ccSource";
export const PI_SOURCE_MANAGED = "managed";
export const PI_SOURCE_OAUTH = "oauth";

/**
 * True when the provider is Pi-owned (managed:* id or OAuth-backed): CC Switch
 * must not write its config or credentials (Pi owns/refreshes OAuth tokens).
 */
export function isPiManagedProvider(
  providerId: string,
  settingsConfig?: unknown,
): boolean {
  if (providerId.startsWith("managed:")) {
    return true;
  }
  if (!settingsConfig || typeof settingsConfig !== "object") {
    return false;
  }
  const source = (settingsConfig as Record<string, unknown>)[PI_SOURCE_FIELD];
  // Rust serde camelCase may also emit `_cc_source` depending on field rename
  const snake = (settingsConfig as Record<string, unknown>)["_cc_source"];
  return (
    source === PI_SOURCE_MANAGED ||
    snake === PI_SOURCE_MANAGED ||
    source === PI_SOURCE_OAUTH ||
    snake === PI_SOURCE_OAUTH
  );
}

export const PI_DEFAULT_CONFIG: PiProviderSettingsConfig = {
  type: "anthropic-messages",
  baseUrl: "https://api.kimi.com/coding/",
  apiKey: "",
  defaultModelId: "k3",
  models: [
    {
      id: "k3",
      name: "K3",
      reasoning: true,
      contextWindow: 1048576,
      maxTokens: 65536,
    },
  ],
};
