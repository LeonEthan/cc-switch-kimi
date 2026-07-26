import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Download, Loader2, Plus, Trash2 } from "lucide-react";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Button } from "@/components/ui/button";
import type { PiModel, PiProviderType } from "@/config/piProviderPresets";
import { piProviderTypeOptions } from "@/config/piProviderPresets";
import { ApiKeySection } from "./shared/ApiKeySection";
import { ModelDropdown } from "./shared/ModelDropdown";
import {
  fetchModelsForConfig,
  showFetchModelsError,
  type FetchedModel,
} from "@/lib/api/model-fetch";
import { normalizeAdditiveProviderKey } from "./helpers/additiveProviderKey";

interface PiFormFieldsProps {
  providerKey: string;
  onProviderKeyChange: (value: string) => void;
  providerKeyDisabled?: boolean;
  providerType: PiProviderType;
  onProviderTypeChange: (value: PiProviderType) => void;
  baseUrl: string;
  onBaseUrlChange: (value: string) => void;
  apiKey: string;
  onApiKeyChange: (value: string) => void;
  shouldShowApiKeyLink?: boolean;
  websiteUrl?: string;
  isPartner?: boolean;
  partnerPromotionKey?: string;
  models: PiModel[];
  onModelsChange: (models: PiModel[]) => void;
  defaultModelId: string;
  onDefaultModelIdChange: (value: string) => void;
  readOnly?: boolean;
}

export function PiFormFields({
  providerKey,
  onProviderKeyChange,
  providerKeyDisabled,
  providerType,
  onProviderTypeChange,
  baseUrl,
  onBaseUrlChange,
  apiKey,
  onApiKeyChange,
  shouldShowApiKeyLink,
  websiteUrl,
  isPartner,
  partnerPromotionKey,
  models,
  onModelsChange,
  defaultModelId,
  onDefaultModelIdChange,
  readOnly,
}: PiFormFieldsProps) {
  const { t } = useTranslation();
  const disabled = !!readOnly;

  const [fetchedModels, setFetchedModels] = useState<FetchedModel[]>([]);
  const [isFetchingModels, setIsFetchingModels] = useState(false);

  const handleFetchModels = useCallback(() => {
    if (!baseUrl || !apiKey) {
      showFetchModelsError(null, t, {
        hasApiKey: !!apiKey,
        hasBaseUrl: !!baseUrl,
      });
      return;
    }
    setIsFetchingModels(true);
    fetchModelsForConfig(baseUrl.trim(), apiKey.trim())
      .then((list) => {
        setFetchedModels(list);
        if (list.length === 0) {
          toast.info(t("providerForm.fetchModelsEmpty"));
        } else {
          toast.success(
            t("providerForm.fetchModelsSuccess", { count: list.length }),
          );
        }
      })
      .catch((err) => {
        console.warn("[ModelFetch] Failed:", err);
        showFetchModelsError(err, t);
      })
      .finally(() => setIsFetchingModels(false));
  }, [baseUrl, apiKey, t]);

  const updateModel = (index: number, patch: Partial<PiModel>) => {
    const next = models.map((m, i) => (i === index ? { ...m, ...patch } : m));
    onModelsChange(next);
  };

  const addModel = () => {
    onModelsChange([
      ...models,
      {
        id: "",
        name: "",
        contextWindow: 262144,
        maxTokens: 32768,
      },
    ]);
  };

  const removeModel = (index: number) => {
    const next = models.filter((_, i) => i !== index);
    onModelsChange(next);
    if (defaultModelId && !next.some((m) => m.id === defaultModelId)) {
      onDefaultModelIdChange(next[0]?.id ?? "");
    }
  };

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor="pi-provider-key">
          {t("pi.form.providerKey", "Provider Key")}
        </Label>
        <Input
          id="pi-provider-key"
          value={providerKey}
          onChange={(e) =>
            onProviderKeyChange(normalizeAdditiveProviderKey(e.target.value))
          }
          placeholder="my-provider"
          disabled={disabled || providerKeyDisabled || readOnly}
        />
        <p className="text-xs text-muted-foreground">
          {t(
            "pi.form.providerKeyHint",
            "Used as providers.<key> in ~/.pi/agent/models.json",
          )}
        </p>
      </div>

      <div className="space-y-2">
        <Label>{t("pi.form.type", "API Protocol")}</Label>
        <Select
          value={providerType}
          onValueChange={(v) => onProviderTypeChange(v as PiProviderType)}
          disabled={disabled}
        >
          <SelectTrigger>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {piProviderTypeOptions.map((opt) => (
              <SelectItem key={opt.value} value={opt.value}>
                {opt.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <div className="space-y-2">
        <Label htmlFor="pi-base-url">{t("pi.form.baseUrl", "Base URL")}</Label>
        <Input
          id="pi-base-url"
          value={baseUrl}
          onChange={(e) => onBaseUrlChange(e.target.value)}
          placeholder="https://api.anthropic.com"
          disabled={disabled}
        />
      </div>

      <ApiKeySection
        value={apiKey}
        onChange={onApiKeyChange}
        disabled={disabled}
        shouldShowLink={!!shouldShowApiKeyLink}
        websiteUrl={websiteUrl ?? ""}
        isPartner={isPartner}
        partnerPromotionKey={partnerPromotionKey}
      />

      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <Label>{t("pi.form.models", "Models")}</Label>
          <div className="flex gap-1">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={handleFetchModels}
              disabled={disabled || isFetchingModels}
            >
              {isFetchingModels ? (
                <Loader2 className="h-3.5 w-3.5 mr-1 animate-spin" />
              ) : (
                <Download className="h-3.5 w-3.5 mr-1" />
              )}
              {t("providerForm.fetchModels", "Fetch Models")}
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={addModel}
              disabled={disabled}
            >
              <Plus className="h-3.5 w-3.5 mr-1" />
              {t("common.add", "Add")}
            </Button>
          </div>
        </div>

        <div className="space-y-3">
          {models.map((model, index) => (
            <div
              key={index}
              className="rounded-md border p-3 space-y-2 bg-muted/20"
            >
              <div className="grid grid-cols-2 gap-2">
                <div className="space-y-1">
                  <Label className="text-xs">
                    {t("pi.form.modelId", "Model ID")}
                  </Label>
                  <div className="flex items-center gap-1">
                    <Input
                      className="flex-1"
                      value={model.id}
                      onChange={(e) =>
                        updateModel(index, { id: e.target.value })
                      }
                      placeholder="claude-fable-5"
                      disabled={disabled}
                    />
                    {!disabled && fetchedModels.length > 0 && (
                      <ModelDropdown
                        models={fetchedModels}
                        onSelect={(modelId) =>
                          updateModel(index, {
                            id: modelId,
                            name: models[index].name || modelId,
                          })
                        }
                      />
                    )}
                  </div>
                </div>
                <div className="space-y-1">
                  <Label className="text-xs">
                    {t("pi.form.modelName", "Display Name")}
                  </Label>
                  <Input
                    value={model.name ?? ""}
                    onChange={(e) =>
                      updateModel(index, { name: e.target.value })
                    }
                    placeholder="Claude Fable 5"
                    disabled={disabled}
                  />
                </div>
              </div>
              <div className="grid grid-cols-2 gap-2">
                <div className="space-y-1">
                  <Label className="text-xs">
                    {t("pi.form.contextWindow", "Context Window")}
                  </Label>
                  <Input
                    type="number"
                    value={model.contextWindow ?? ""}
                    onChange={(e) =>
                      updateModel(index, {
                        contextWindow: e.target.value
                          ? Number(e.target.value)
                          : undefined,
                      })
                    }
                    placeholder="1000000"
                    disabled={disabled}
                  />
                </div>
                <div className="space-y-1">
                  <Label className="text-xs">
                    {t("pi.form.maxTokens", "Max Output Tokens")}
                  </Label>
                  <Input
                    type="number"
                    value={model.maxTokens ?? ""}
                    onChange={(e) =>
                      updateModel(index, {
                        maxTokens: e.target.value
                          ? Number(e.target.value)
                          : undefined,
                      })
                    }
                    placeholder="128000"
                    disabled={disabled}
                  />
                </div>
              </div>
              <div className="flex justify-end">
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => removeModel(index)}
                  disabled={disabled || models.length <= 1}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </div>
            </div>
          ))}
        </div>
      </div>

      <div className="space-y-2">
        <Label>{t("pi.form.defaultModel", "Default Model")}</Label>
        <Select
          value={defaultModelId || models[0]?.id || ""}
          onValueChange={onDefaultModelIdChange}
          disabled={disabled || models.length === 0}
        >
          <SelectTrigger>
            <SelectValue
              placeholder={t("pi.form.selectModel", "Select model")}
            />
          </SelectTrigger>
          <SelectContent>
            {models
              .filter((m) => m.id.trim())
              .map((m) => (
                <SelectItem key={m.id} value={m.id}>
                  {m.name || m.id}
                </SelectItem>
              ))}
          </SelectContent>
        </Select>
        <p className="text-xs text-muted-foreground">
          {t(
            "pi.form.defaultModelHint",
            "Written to settings.json defaultModel when this provider is enabled",
          )}
        </p>
      </div>
    </div>
  );
}
