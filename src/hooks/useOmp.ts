import { useQuery } from "@tanstack/react-query";
import { providersApi } from "@/lib/api";
import { invoke } from "@tauri-apps/api/core";

export const ompKeys = {
  liveProviderIds: ["ompLiveProviderIds"] as const,
  modelRoles: ["ompModelRoles"] as const,
  version: ["ompVersion"] as const,
};

/** Live provider ids present in ~/.omp/agent/models.yml */
export function useOmpLiveProviderIds(enabled: boolean) {
  return useQuery({
    queryKey: ompKeys.liveProviderIds,
    queryFn: () => providersApi.getOmpLiveProviderIds(),
    enabled,
  });
}

/** Role → "<providerKey>/<modelId>" selector map (config.yml modelRoles). */
export function useOmpModelRoles(enabled: boolean) {
  return useQuery({
    queryKey: ompKeys.modelRoles,
    queryFn: () => providersApi.getOmpModelRoles(),
    enabled,
  });
}

/** Detected Omp CLI version; null when the `omp` binary is not installed. */
export function useOmpVersion(enabled: boolean) {
  return useQuery({
    queryKey: ompKeys.version,
    queryFn: () => invoke<string | null>("getOmpVersion"),
    enabled,
  });
}
