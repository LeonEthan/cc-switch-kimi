import { useQuery } from "@tanstack/react-query";
import { providersApi } from "@/lib/api";
import { invoke } from "@tauri-apps/api/core";

export const piKeys = {
  liveProviderIds: ["piLiveProviderIds"] as const,
  defaultProviderId: ["piDefaultProviderId"] as const,
  defaultModel: ["piDefaultModel"] as const,
  version: ["piVersion"] as const,
};

/** Live provider ids present in ~/.pi/agent/models.json */
export function usePiLiveProviderIds(enabled: boolean) {
  return useQuery({
    queryKey: piKeys.liveProviderIds,
    queryFn: () => providersApi.getPiLiveProviderIds(),
    enabled,
  });
}

/** Provider id that owns live `defaultProvider` (selection / "current"). */
export function usePiDefaultProviderId(enabled: boolean) {
  return useQuery({
    queryKey: piKeys.defaultProviderId,
    queryFn: () => invoke<string | null>("getPiDefaultProviderId"),
    enabled,
  });
}

export function usePiDefaultModel(enabled: boolean) {
  return useQuery({
    queryKey: piKeys.defaultModel,
    queryFn: () => invoke<string | null>("getPiDefaultModel"),
    enabled,
  });
}

/** Detected Pi CLI version; null when the `pi` binary is not installed. */
export function usePiVersion(enabled: boolean) {
  return useQuery({
    queryKey: piKeys.version,
    queryFn: () => invoke<string | null>("getPiVersion"),
    enabled,
  });
}
