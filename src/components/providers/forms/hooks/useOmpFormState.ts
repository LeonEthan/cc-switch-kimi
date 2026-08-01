import { useState, useCallback, useMemo } from "react";
import type { AppId } from "@/lib/api";
import { useProvidersQuery } from "@/lib/query/queries";
import {
  OMP_DEFAULT_CONFIG,
  type OmpModel,
  type OmpProviderSettingsConfig,
  type OmpProviderType,
} from "@/config/ompProviderPresets";

interface UseOmpFormStateParams {
  initialData?: {
    settingsConfig?: Record<string, unknown>;
  };
  appId: AppId;
  providerId?: string;
  onSettingsConfigChange: (config: string) => void;
  getSettingsConfig: () => string;
}

function parseField<T>(
  initialData: UseOmpFormStateParams["initialData"],
  field: string,
  fallback: T,
): T {
  try {
    if (initialData?.settingsConfig) {
      const value = initialData.settingsConfig[field];
      return (value as T) ?? fallback;
    }
    return (
      ((OMP_DEFAULT_CONFIG as unknown as Record<string, unknown>)[
        field
      ] as T) ?? fallback
    );
  } catch {
    return fallback;
  }
}

export function useOmpFormState({
  initialData,
  appId,
  providerId,
  onSettingsConfigChange,
  getSettingsConfig,
}: UseOmpFormStateParams) {
  const { data: providersData } = useProvidersQuery("omp");
  const existingKeys = useMemo(() => {
    if (!providersData?.providers) return [];
    return Object.keys(providersData.providers).filter((k) => k !== providerId);
  }, [providersData?.providers, providerId]);

  const [providerKey, setProviderKey] = useState(() => {
    if (appId !== "omp") return "";
    return providerId || "";
  });

  const [providerType, setProviderType] = useState<OmpProviderType>(() => {
    if (appId !== "omp") return "anthropic-messages";
    return parseField(initialData, "api", "anthropic-messages");
  });

  const [baseUrl, setBaseUrl] = useState(() => {
    if (appId !== "omp") return "";
    return (
      parseField(initialData, "baseUrl", "") ||
      parseField(initialData, "base_url", "")
    );
  });

  const [apiKey, setApiKey] = useState(() => {
    if (appId !== "omp") return "";
    return (
      parseField(initialData, "apiKey", "") ||
      parseField(initialData, "api_key", "")
    );
  });

  const [authHeader, setAuthHeader] = useState<boolean>(() => {
    if (appId !== "omp") return false;
    return parseField(initialData, "authHeader", false);
  });

  const [models, setModels] = useState<OmpModel[]>(() => {
    if (appId !== "omp") return [];
    return parseField(initialData, "models", OMP_DEFAULT_CONFIG.models);
  });

  const [defaultModelId, setDefaultModelId] = useState(() => {
    if (appId !== "omp") return "";
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
    (patch: Partial<OmpProviderSettingsConfig>) => {
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
    (api: OmpProviderType) => {
      setProviderType(api);
      writeConfig({ api });
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

  const handleAuthHeaderChange = useCallback(
    (value: boolean) => {
      setAuthHeader(value);
      writeConfig({ authHeader: value });
    },
    [writeConfig],
  );

  const handleModelsChange = useCallback(
    (nextModels: OmpModel[]) => {
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
    (config?: Partial<OmpProviderSettingsConfig>) => {
      const next = { ...OMP_DEFAULT_CONFIG, ...config };
      setProviderType(next.api);
      setBaseUrl(next.baseUrl ?? "");
      setApiKey(next.apiKey ?? "");
      setAuthHeader(next.authHeader ?? false);
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
    authHeader,
    models,
    defaultModelId,
    existingKeys,
    handleTypeChange,
    handleBaseUrlChange,
    handleApiKeyChange,
    handleAuthHeaderChange,
    handleModelsChange,
    handleDefaultModelIdChange,
    resetState,
  };
}
