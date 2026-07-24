/**
 * Kimi Code CLI provider presets.
 * Live format: ~/.kimi-code/config.toml [providers.*] + [models.*]
 */
import type { ProviderCategory } from "../types";
import type { PresetTheme, TemplateValueConfig } from "./claudeProviderPresets";

export interface KimiCodeModel {
  id: string;
  model?: string;
  maxContextSize?: number;
  maxInputSize?: number;
  maxOutputSize?: number;
  displayName?: string;
  capabilities?: string[];
  supportEfforts?: string[];
  defaultEffort?: string;
}

export type KimiCodeProviderType =
  | "kimi"
  | "openai"
  | "openai_responses"
  | "anthropic"
  | "google-genai"
  | "vertexai";

export interface KimiCodeProviderSettingsConfig {
  type: KimiCodeProviderType;
  apiKey?: string;
  baseUrl?: string;
  models: KimiCodeModel[];
  defaultModelId?: string;
  customHeaders?: Record<string, string>;
}

export interface KimiCodeProviderPreset {
  name: string;
  nameKey?: string;
  websiteUrl: string;
  apiKeyUrl?: string;
  settingsConfig: KimiCodeProviderSettingsConfig;
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

const KIMI_CAPABILITIES = [
  "thinking",
  "always_thinking",
  "image_in",
  "video_in",
  "tool_use",
];

export const kimiCodeProviderTypeOptions: Array<{
  value: KimiCodeProviderType;
  label: string;
}> = [
  { value: "kimi", label: "Kimi (OpenAI-compatible)" },
  { value: "openai", label: "OpenAI Chat Completions" },
  { value: "openai_responses", label: "OpenAI Responses" },
  { value: "anthropic", label: "Anthropic Messages" },
  { value: "google-genai", label: "Google GenAI" },
  { value: "vertexai", label: "Google Vertex AI" },
];

export const kimiCodeProviderPresets: KimiCodeProviderPreset[] = [
  {
    name: "Kimi For Coding",
    websiteUrl: "https://www.kimi.com/code/?aff=cc-switch",
    apiKeyUrl: "https://www.kimi.com/code/?aff=cc-switch",
    category: "cn_official",
    icon: "kimi",
    settingsConfig: {
      type: "kimi",
      baseUrl: "https://api.kimi.com/coding/v1",
      apiKey: "",
      defaultModelId: "k3",
      models: [
        {
          id: "k3",
          model: "k3",
          displayName: "K3",
          maxContextSize: 1048576,
          capabilities: KIMI_CAPABILITIES,
          supportEfforts: ["low", "high", "max"],
          defaultEffort: "high",
        },
        {
          id: "kimi-for-coding",
          model: "kimi-for-coding",
          displayName: "K2.7 Coding",
          maxContextSize: 262144,
          capabilities: KIMI_CAPABILITIES,
        },
        {
          id: "kimi-for-coding-highspeed",
          model: "kimi-for-coding-highspeed",
          displayName: "K2.7 Coding Highspeed",
          maxContextSize: 262144,
          capabilities: KIMI_CAPABILITIES,
        },
      ],
    },
  },
  {
    name: "Kimi Platform",
    websiteUrl: "https://platform.kimi.com?aff=cc-switch",
    apiKeyUrl: "https://platform.kimi.com/console/api-keys?aff=cc-switch",
    category: "cn_official",
    icon: "kimi",
    settingsConfig: {
      type: "kimi",
      baseUrl: "https://api.moonshot.cn/v1",
      apiKey: "",
      defaultModelId: "kimi-k2.7-code",
      models: [
        {
          id: "kimi-k2.7-code",
          model: "kimi-k2.7-code",
          displayName: "Kimi K2.7 Code",
          maxContextSize: 262144,
          capabilities: KIMI_CAPABILITIES,
        },
        {
          id: "kimi-k3",
          model: "kimi-k3",
          displayName: "Kimi K3",
          maxContextSize: 1048576,
          capabilities: KIMI_CAPABILITIES,
          supportEfforts: ["low", "high", "max"],
          defaultEffort: "high",
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
      type: "openai",
      baseUrl: "https://api.openai.com/v1",
      apiKey: "",
      defaultModelId: "gpt-4.1",
      models: [
        {
          id: "gpt-4.1",
          model: "gpt-4.1",
          displayName: "GPT-4.1",
          maxContextSize: 1047576,
          capabilities: ["tool_use", "image_in"],
        },
      ],
    },
  },
];

/** Marker field written by Rust `get_providers` for managed OAuth entries. */
export const KIMICODE_SOURCE_FIELD = "_ccSource";
export const KIMICODE_SOURCE_MANAGED = "managed";

/** True when the provider is the OAuth-managed `managed:*` account. */
export function isKimiCodeManagedProvider(
  providerId: string,
  settingsConfig?: unknown,
): boolean {
  if (providerId === "managed:kimi-code" || providerId.startsWith("managed:")) {
    return true;
  }
  if (!settingsConfig || typeof settingsConfig !== "object") {
    return false;
  }
  const source = (settingsConfig as Record<string, unknown>)[
    KIMICODE_SOURCE_FIELD
  ];
  // Rust serde camelCase may also emit `_cc_source` depending on field rename
  const snake = (settingsConfig as Record<string, unknown>)["_cc_source"];
  return source === KIMICODE_SOURCE_MANAGED || snake === KIMICODE_SOURCE_MANAGED;
}

export const KIMICODE_DEFAULT_CONFIG: KimiCodeProviderSettingsConfig = {
  type: "kimi",
  baseUrl: "https://api.kimi.com/coding/v1",
  apiKey: "",
  defaultModelId: "k3",
  models: [
    {
      id: "k3",
      model: "k3",
      displayName: "K3",
      maxContextSize: 1048576,
      capabilities: KIMI_CAPABILITIES,
      supportEfforts: ["low", "high", "max"],
      defaultEffort: "high",
    },
  ],
};
