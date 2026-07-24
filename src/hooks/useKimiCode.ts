import { useQuery } from "@tanstack/react-query";
import { providersApi } from "@/lib/api";
import { invoke } from "@tauri-apps/api/core";

export const kimiCodeKeys = {
  liveProviderIds: ["kimicodeLiveProviderIds"] as const,
  defaultProviderId: ["kimicodeDefaultProviderId"] as const,
  defaultModel: ["kimicodeDefaultModel"] as const,
};

/** Live provider ids present in ~/.kimi-code/config.toml */
export function useKimiCodeLiveProviderIds(enabled: boolean) {
  return useQuery({
    queryKey: kimiCodeKeys.liveProviderIds,
    queryFn: () => providersApi.getKimiCodeLiveProviderIds(),
    enabled,
  });
}

/** Provider id that owns live `default_model` (selection / "current"). */
export function useKimiCodeDefaultProviderId(enabled: boolean) {
  return useQuery({
    queryKey: kimiCodeKeys.defaultProviderId,
    queryFn: () =>
      invoke<string | null>("get_kimicode_default_provider_id"),
    enabled,
  });
}

export function useKimiCodeDefaultModel(enabled: boolean) {
  return useQuery({
    queryKey: kimiCodeKeys.defaultModel,
    queryFn: () => invoke<string | null>("get_kimicode_default_model"),
    enabled,
  });
}
