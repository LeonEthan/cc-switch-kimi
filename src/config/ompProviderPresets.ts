/**
 * Omp (Oh My Pi) provider presets.
 * Live format: ~/.omp/agent/models.yml `providers.<key>` (YAML), selection in
 * config.yml `modelRoles.<role> = "<key>/<modelId>"`. The DB fragment uses
 * camelCase and stores the protocol in `api` (unlike Pi, whose DB fragment
 * uses `type` and Rust maps it to models.json's `api` on write).
 */
import type { ProviderCategory } from "../types";
import type { PresetTheme, TemplateValueConfig } from "./claudeProviderPresets";

export interface OmpModel {
  id: string;
  name?: string;
  /** Per-model API override (defaults to the provider's `api`). */
  api?: string;
  reasoning?: boolean;
  input?: string[];
  contextWindow?: number;
  maxTokens?: number;
}

/** Omp's KnownApi protocols offered in the form dropdown. */
export type OmpProviderType =
  | "openai-completions"
  | "openai-responses"
  | "openai-codex-responses"
  | "azure-openai-responses"
  | "anthropic-messages"
  | "google-generative-ai"
  | "google-vertex";

export interface OmpProviderSettingsConfig {
  api: OmpProviderType;
  apiKey?: string;
  baseUrl?: string;
  /** models.yml `authHeader`: send `Authorization: Bearer <apiKey>`. */
  authHeader?: boolean;
  models: OmpModel[];
  defaultModelId?: string;
  displayName?: string;
  customHeaders?: Record<string, string>;
}

export interface OmpProviderPreset {
  name: string;
  nameKey?: string;
  websiteUrl: string;
  apiKeyUrl?: string;
  settingsConfig: OmpProviderSettingsConfig;
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

export const ompProviderTypeOptions: Array<{
  value: OmpProviderType;
  label: string;
}> = [
  { value: "openai-completions", label: "OpenAI Chat Completions" },
  { value: "openai-responses", label: "OpenAI Responses" },
  { value: "openai-codex-responses", label: "OpenAI Codex Responses" },
  { value: "azure-openai-responses", label: "Azure OpenAI Responses" },
  { value: "anthropic-messages", label: "Anthropic Messages" },
  { value: "google-generative-ai", label: "Google Generative AI" },
  { value: "google-vertex", label: "Google Vertex AI" },
];

export const ompProviderPresets: OmpProviderPreset[] = [
  {
    name: "Kimi For Coding",
    websiteUrl: "https://www.kimi.com/code/?aff=cc-switch",
    apiKeyUrl: "https://www.kimi.com/code/?aff=cc-switch",
    category: "cn_official",
    icon: "kimi",
    settingsConfig: {
      // Anthropic-compatible endpoint; OpenAI-compatible is at /coding/v1
      api: "anthropic-messages",
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
    name: "Z.AI (GLM Coding Plan)",
    websiteUrl: "https://z.ai",
    apiKeyUrl: "https://z.ai/subscribe?ic=8JVLJQFSKB",
    category: "cn_official",
    icon: "zhipu",
    iconColor: "#0F62FE",
    settingsConfig: {
      api: "anthropic-messages",
      baseUrl: "https://api.z.ai/api/anthropic",
      apiKey: "",
      defaultModelId: "glm-5.1",
      models: [
        {
          id: "glm-5.1",
          name: "GLM 5.1",
          reasoning: true,
          contextWindow: 204800,
          maxTokens: 128000,
        },
      ],
    },
  },
  {
    name: "MiniMax Coding Plan",
    websiteUrl: "https://platform.minimaxi.com",
    apiKeyUrl: "https://platform.minimaxi.com/subscribe/coding-plan",
    category: "cn_official",
    partnerPromotionKey: "minimax_cn",
    icon: "minimax",
    iconColor: "#FF6B6B",
    settingsConfig: {
      api: "anthropic-messages",
      baseUrl: "https://api.minimaxi.com/anthropic",
      apiKey: "",
      defaultModelId: "MiniMax-M2.7",
      models: [
        {
          id: "MiniMax-M2.7",
          name: "MiniMax M2.7",
          reasoning: true,
          contextWindow: 204800,
          maxTokens: 128000,
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
      api: "anthropic-messages",
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
    name: "OpenRouter",
    websiteUrl: "https://openrouter.ai",
    apiKeyUrl: "https://openrouter.ai/keys",
    category: "aggregator",
    icon: "openrouter",
    iconColor: "#6566F1",
    settingsConfig: {
      api: "openai-completions",
      baseUrl: "https://openrouter.ai/api/v1",
      apiKey: "",
      defaultModelId: "anthropic/claude-sonnet-5",
      models: [
        {
          id: "anthropic/claude-sonnet-5",
          name: "Claude Sonnet 5",
          reasoning: true,
          contextWindow: 1000000,
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
      api: "openai-completions",
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
export const OMP_SOURCE_FIELD = "_ccSource";
export const OMP_SOURCE_MANAGED = "managed";
export const OMP_SOURCE_OAUTH = "oauth";

/**
 * True when the provider is Omp-owned (`_ccSource: managed|oauth`): CC Switch
 * must not write its config or credentials (Omp owns/refreshes OAuth tokens
 * in agent.db, which is strictly read-only).
 *
 * Accepts all three spellings of the marker key — `_ccSource` (what omp's
 * backend writes), `_cc_source` and `ccSource` (serde rename quirks seen in
 * the Pi path) — for robustness.
 */
export function isOmpManagedProvider(
  _providerId: string,
  settingsConfig?: unknown,
): boolean {
  if (!settingsConfig || typeof settingsConfig !== "object") {
    return false;
  }
  const record = settingsConfig as Record<string, unknown>;
  const source =
    record[OMP_SOURCE_FIELD] ?? record["_cc_source"] ?? record["ccSource"];
  return source === OMP_SOURCE_MANAGED || source === OMP_SOURCE_OAUTH;
}

export const OMP_DEFAULT_CONFIG: OmpProviderSettingsConfig = {
  api: "anthropic-messages",
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
