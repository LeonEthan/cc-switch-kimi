import { useState, useCallback, useMemo } from "react";
import type { AppId } from "@/lib/api";
import { useProvidersQuery } from "@/lib/query/queries";
import {
  PI_DEFAULT_CONFIG,
  type PiModel,
  type PiProviderSettingsConfig,
  type PiProviderType,
} from "@/config/piProviderPresets";

interface UsePiFormStateParams {
  initialData?: {
    settingsConfig?: Record<string, unknown>;
  };
  appId: AppId;
  providerId?: string;
  onSettingsConfigChange: (config: string) => void;
  getSettingsConfig: () => string;
}

function parseField<T>(
  initialData: UsePiFormStateParams["initialData"],
  field: string,
  fallback: T,
): T {
  try {
    if (initialData?.settingsConfig) {
      const value = initialData.settingsConfig[field];
      return (value as T) ?? fallback;
    }
    return (
      ((PI_DEFAULT_CONFIG as unknown as Record<string, unknown>)[field] as T) ??
      fallback
    );
  } catch {
    return fallback;
  }
}

export function usePiFormState({
  initialData,
  appId,
  providerId,
  onSettingsConfigChange,
  getSettingsConfig,
}: UsePiFormStateParams) {
  const { data: providersData } = useProvidersQuery("pi");
  const existingKeys = useMemo(() => {
    if (!providersData?.providers) return [];
    return Object.keys(providersData.providers).filter((k) => k !== providerId);
  }, [providersData?.providers, providerId]);

  const [providerKey, setProviderKey] = useState(() => {
    if (appId !== "pi") return "";
    return providerId || "";
  });

  const [providerType, setProviderType] = useState<PiProviderType>(() => {
    if (appId !== "pi") return "anthropic-messages";
    return parseField(initialData, "type", "anthropic-messages");
  });

  const [baseUrl, setBaseUrl] = useState(() => {
    if (appId !== "pi") return "";
    return (
      parseField(initialData, "baseUrl", "") ||
      parseField(initialData, "base_url", "")
    );
  });

  const [apiKey, setApiKey] = useState(() => {
    if (appId !== "pi") return "";
    return (
      parseField(initialData, "apiKey", "") ||
      parseField(initialData, "api_key", "")
    );
  });

  const [models, setModels] = useState<PiModel[]>(() => {
    if (appId !== "pi") return [];
    return parseField(initialData, "models", PI_DEFAULT_CONFIG.models);
  });

  const [defaultModelId, setDefaultModelId] = useState(() => {
    if (appId !== "pi") return "";
    return (
      parseField(initialData, "defaultModelId", "") ||
      parseField(initialData, "default_model_id", "") ||
      models[0]?.id ||
      ""
    );
  });

  /**
   * Single source of truth: merge `patch` into the form's settingsConfig JSON.
   * Do not re-apply React field snapshots — that clobbers JsonEditor edits.
   */
  const writeConfig = useCallback(
    (patch: Partial<PiProviderSettingsConfig>) => {
      let current: Record<string, unknown> = {};
      try {
        const parsed = JSON.parse(getSettingsConfig() || "{}");
        if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
          current = parsed as Record<string, unknown>;
        }
      } catch {
        current = {};
      }
      const next = { ...current, ...patch };
      onSettingsConfigChange(JSON.stringify(next, null, 2));
    },
    [getSettingsConfig, onSettingsConfigChange],
  );

  const handleTypeChange = useCallback(
    (type: PiProviderType) => {
      setProviderType(type);
      writeConfig({ type });
    },
    [writeConfig],
  );

  const handleBaseUrlChange = useCallback(
    (value: string) => {
      setBaseUrl(value);
      writeConfig({ baseUrl: value });
    },
    [writeConfig],
  );

  const handleApiKeyChange = useCallback(
    (value: string) => {
      setApiKey(value);
      writeConfig({ apiKey: value });
    },
    [writeConfig],
  );

  const handleModelsChange = useCallback(
    (nextModels: PiModel[]) => {
      setModels(nextModels);
      const nextDefault =
        defaultModelId && nextModels.some((m) => m.id === defaultModelId)
          ? defaultModelId
          : (nextModels[0]?.id ?? "");
      setDefaultModelId(nextDefault);
      writeConfig({ models: nextModels, defaultModelId: nextDefault });
    },
    [defaultModelId, writeConfig],
  );

  const handleDefaultModelIdChange = useCallback(
    (value: string) => {
      setDefaultModelId(value);
      writeConfig({ defaultModelId: value });
    },
    [writeConfig],
  );

  const resetState = useCallback(
    (config?: Partial<PiProviderSettingsConfig>) => {
      const next = { ...PI_DEFAULT_CONFIG, ...config };
      setProviderType(next.type);
      setBaseUrl(next.baseUrl ?? "");
      setApiKey(next.apiKey ?? "");
      setModels(next.models ?? []);
      setDefaultModelId(next.defaultModelId ?? next.models?.[0]?.id ?? "");
      onSettingsConfigChange(JSON.stringify(next, null, 2));
    },
    [onSettingsConfigChange],
  );

  return {
    providerKey,
    setProviderKey,
    providerType,
    baseUrl,
    apiKey,
    models,
    defaultModelId,
    existingKeys,
    handleTypeChange,
    handleBaseUrlChange,
    handleApiKeyChange,
    handleModelsChange,
    handleDefaultModelIdChange,
    resetState,
  };
}
