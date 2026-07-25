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
import type {
  KimiCodeModel,
  KimiCodeProviderType,
} from "@/config/kimiCodeProviderPresets";
import { kimiCodeProviderTypeOptions } from "@/config/kimiCodeProviderPresets";
import { ApiKeySection } from "./shared/ApiKeySection";
import { ModelDropdown } from "./shared/ModelDropdown";
import {
  fetchModelsForConfig,
  showFetchModelsError,
  type FetchedModel,
} from "@/lib/api/model-fetch";
import { normalizeAdditiveProviderKey } from "./helpers/additiveProviderKey";

interface KimiCodeFormFieldsProps {
  providerKey: string;
  onProviderKeyChange: (value: string) => void;
  providerKeyDisabled?: boolean;
  providerType: KimiCodeProviderType;
  onProviderTypeChange: (value: KimiCodeProviderType) => void;
  baseUrl: string;
  onBaseUrlChange: (value: string) => void;
  apiKey: string;
  onApiKeyChange: (value: string) => void;
  shouldShowApiKeyLink?: boolean;
  websiteUrl?: string;
  isPartner?: boolean;
  partnerPromotionKey?: string;
  models: KimiCodeModel[];
  onModelsChange: (models: KimiCodeModel[]) => void;
  defaultModelId: string;
  onDefaultModelIdChange: (value: string) => void;
  readOnly?: boolean;
}

export function KimiCodeFormFields({
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
}: KimiCodeFormFieldsProps) {
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

  const updateModel = (index: number, patch: Partial<KimiCodeModel>) => {
    const next = models.map((m, i) => (i === index ? { ...m, ...patch } : m));
    onModelsChange(next);
  };

  const addModel = () => {
    onModelsChange([
      ...models,
      {
        id: "",
        model: "",
        displayName: "",
        maxContextSize: 262144,
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
        <Label htmlFor="kimicode-provider-key">
          {t("kimicode.form.providerKey", "Provider Key")}
        </Label>
        <Input
          id="kimicode-provider-key"
          value={providerKey}
          onChange={(e) =>
            onProviderKeyChange(normalizeAdditiveProviderKey(e.target.value))
          }
          placeholder="acc-a"
          disabled={disabled || providerKeyDisabled || readOnly}
        />
        <p className="text-xs text-muted-foreground">
          {t(
            "kimicode.form.providerKeyHint",
            "Used as [providers.<key>] in ~/.kimi-code/config.toml",
          )}
        </p>
      </div>

      <div className="space-y-2">
        <Label>{t("kimicode.form.type", "Provider Type")}</Label>
        <Select
          value={providerType}
          onValueChange={(v) => onProviderTypeChange(v as KimiCodeProviderType)}
          disabled={disabled}
        >
          <SelectTrigger>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {kimiCodeProviderTypeOptions.map((opt) => (
              <SelectItem key={opt.value} value={opt.value}>
                {opt.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <div className="space-y-2">
        <Label htmlFor="kimicode-base-url">
          {t("kimicode.form.baseUrl", "Base URL")}
        </Label>
        <Input
          id="kimicode-base-url"
          value={baseUrl}
          onChange={(e) => onBaseUrlChange(e.target.value)}
          placeholder="https://api.kimi.com/coding/v1"
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
          <Label>{t("kimicode.form.models", "Models")}</Label>
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
                    {t("kimicode.form.modelId", "Alias / ID")}
                  </Label>
                  <div className="flex items-center gap-1">
                    <Input
                      className="flex-1"
                      value={model.id}
                      onChange={(e) =>
                        updateModel(index, { id: e.target.value })
                      }
                      placeholder="k3"
                      disabled={disabled}
                    />
                    {!disabled && fetchedModels.length > 0 && (
                      <ModelDropdown
                        models={fetchedModels}
                        onSelect={(modelId) =>
                          updateModel(index, {
                            id: modelId,
                            model: modelId,
                            displayName: models[index].displayName || modelId,
                          })
                        }
                      />
                    )}
                  </div>
                </div>
                <div className="space-y-1">
                  <Label className="text-xs">
                    {t("kimicode.form.modelApi", "API Model")}
                  </Label>
                  <Input
                    value={model.model ?? model.id}
                    onChange={(e) =>
                      updateModel(index, { model: e.target.value })
                    }
                    placeholder="k3"
                    disabled={disabled}
                  />
                </div>
              </div>
              <div className="grid grid-cols-2 gap-2">
                <div className="space-y-1">
                  <Label className="text-xs">
                    {t("kimicode.form.displayName", "Display Name")}
                  </Label>
                  <Input
                    value={model.displayName ?? ""}
                    onChange={(e) =>
                      updateModel(index, { displayName: e.target.value })
                    }
                    placeholder="K3"
                    disabled={disabled}
                  />
                </div>
                <div className="space-y-1">
                  <Label className="text-xs">
                    {t("kimicode.form.maxContext", "Max Context")}
                  </Label>
                  <Input
                    type="number"
                    value={model.maxContextSize ?? ""}
                    onChange={(e) =>
                      updateModel(index, {
                        maxContextSize: e.target.value
                          ? Number(e.target.value)
                          : undefined,
                      })
                    }
                    placeholder="1048576"
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
        <Label>{t("kimicode.form.defaultModel", "Default Model")}</Label>
        <Select
          value={defaultModelId || models[0]?.id || ""}
          onValueChange={onDefaultModelIdChange}
          disabled={disabled || models.length === 0}
        >
          <SelectTrigger>
            <SelectValue
              placeholder={t("kimicode.form.selectModel", "Select model")}
            />
          </SelectTrigger>
          <SelectContent>
            {models
              .filter((m) => m.id.trim())
              .map((m) => (
                <SelectItem key={m.id} value={m.id}>
                  {m.displayName || m.id}
                </SelectItem>
              ))}
          </SelectContent>
        </Select>
        <p className="text-xs text-muted-foreground">
          {t(
            "kimicode.form.defaultModelHint",
            "Written to default_model when this provider is switched on",
          )}
        </p>
      </div>
    </div>
  );
}
