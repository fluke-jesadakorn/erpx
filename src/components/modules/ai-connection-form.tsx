"use client";
import { TimeText } from "@/components/time/time-preference-context";

import {
  AlertTriangle,
  CheckCircle2,
  ExternalLink,
  Info,
  RefreshCw,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import {
  aiPresets,
  presetForUrl,
  supportedModelProtocol,
  supportedModels,
  unsupportedModelMessage,
  type AiPreset,
} from "@/lib/ai/providers";
import { workFetch } from "@/lib/work-fetch";
import type { AiConnectionView } from "@/server/ai-providers";
import styles from "./ai-connection-form.module.css";

function comparableBaseUrl(value: string) {
  try {
    return new URL(value).toString().replace(/\/$/, "");
  } catch {
    return value.trim().replace(/\/+$/, "");
  }
}

export function AiConnectionForm({
  edit,
  base,
  pending,
  error,
  keyStorageReady,
  save,
  onCancel,
}: {
  edit: AiConnectionView | "new";
  base: string;
  pending: boolean;
  error: string;
  keyStorageReady: boolean;
  save: (input: unknown) => Promise<void>;
  onCancel: () => void;
}) {
  const row = edit === "new" ? null : edit;
  const formId = useId();
  const modelListId = `${formId}-models`;
  const apiKeyId = `${formId}-api-key`;
  const apiKeyHelpId = `${formId}-api-key-help`;
  const [preset, setPreset] = useState<AiPreset>(
    row?.provider_preset ?? "opencode-zen",
  );
  const [baseUrl, setBaseUrl] = useState(
    row?.base_url ?? aiPresets["opencode-zen"].baseUrl,
  );
  const [name, setName] = useState(row?.name ?? "OpenCode Zen");
  const [authMode, setAuthMode] = useState(row?.auth_mode ?? "bearer");
  const [apiKey, setApiKey] = useState("");
  const [removeKey, setRemoveKey] = useState(false);
  const [textModel, setTextModel] = useState(row?.text_model ?? "");
  const [sameModel, setSameModel] = useState(
    !!row?.image_model && row.image_model === row.text_model,
  );
  const [imageModel, setImageModel] = useState(row?.image_model ?? "");
  const [models, setModels] = useState<string[]>([]);
  const [totalModels, setTotalModels] = useState(0);
  const [fetchedAt, setFetchedAt] = useState("");
  const [discoverError, setDiscoverError] = useState("");
  const [loading, setLoading] = useState(false);
  const generation = useRef(0);
  const controller = useRef<AbortController | null>(null);

  useEffect(
    () => () => {
      generation.current++;
      controller.current?.abort();
    },
    [],
  );

  function invalidate() {
    generation.current++;
    controller.current?.abort();
    setLoading(false);
    setModels([]);
    setTotalModels(0);
    setFetchedAt("");
    setDiscoverError("");
  }

  function choosePreset(next: AiPreset) {
    invalidate();
    setPreset(next);
    setBaseUrl(aiPresets[next].baseUrl);
    setName(aiPresets[next].name);
    setApiKey("");
    setRemoveKey(false);
    setAuthMode("bearer");
    setTextModel("");
    setImageModel("");
    setSameModel(false);
  }

  function chooseModel(kind: "text" | "image", id: string) {
    if (kind === "text") {
      setTextModel(id);
      if (sameModel) setImageModel(id);
    } else {
      setImageModel(id);
    }
  }

  async function discover() {
    controller.current?.abort();
    const abort = new AbortController();
    controller.current = abort;
    const requestId = ++generation.current;
    setLoading(true);
    setDiscoverError("");
    try {
      const response = await workFetch(
        `/api/v1${base}/ai-providers/discover`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          signal: abort.signal,
          cache: "no-store",
          body: JSON.stringify({
            baseUrl,
            authMode,
            apiKey,
            removeKey,
            connectionId: row?.id,
          }),
        },
      );
      const result = (await response.json()) as {
        data: { models: string[]; totalModels: number; fetchedAt: string };
        error?: { message: string };
      };
      if (!response.ok)
        throw Error(result.error?.message ?? "Model discovery failed.");
      if (requestId === generation.current) {
        setModels(result.data.models);
        setTotalModels(result.data.totalModels);
        setFetchedAt(result.data.fetchedAt);
      }
    } catch (caught) {
      if (requestId === generation.current && !abort.signal.aborted)
        setDiscoverError(
          caught instanceof Error ? caught.message : "Model discovery failed.",
        );
    } finally {
      if (requestId === generation.current) setLoading(false);
    }
  }

  const unsupported = (id: string) =>
    !!id.trim() && !supportedModelProtocol(baseUrl, id);
  const selectableModels = supportedModels(baseUrl, models);
  const invalid =
    !textModel.trim() || unsupported(textModel) || unsupported(imageModel);
  const unsupportedCount = Math.max(0, totalModels - selectableModels.length);
  const changedCredentialUrl =
    !!row?.hasKey &&
    comparableBaseUrl(baseUrl) !== comparableBaseUrl(row.base_url);
  const needsKeyForChangedUrl =
    changedCredentialUrl && authMode === "bearer" && !removeKey && !apiKey.trim();
  const needsCredentialStorage =
    authMode === "bearer" && !!apiKey.trim() && !keyStorageReady;
  const apiKeyPlaceholder =
    authMode === "none"
      ? "No key sent"
      : removeKey
        ? "Saved key will be removed"
        : changedCredentialUrl
          ? "Enter a key for this URL"
          : row?.hasKey
            ? "Saved key stays in place"
            : "Paste API key";
  const apiKeyHelp =
    authMode === "none"
      ? row?.hasKey
        ? "This draft will use an unauthenticated endpoint and will not retain the saved key."
        : "This draft will use an unauthenticated endpoint and will not store a key."
      : removeKey
        ? "This draft will be saved without a credential."
        : needsKeyForChangedUrl
          ? "Enter a new API key when changing the provider URL."
          : needsCredentialStorage
            ? "Encrypted credential storage must be configured before a new key can be saved."
            : apiKey
              ? row?.hasKey
                ? "This key will replace the credential in the new draft."
                : "This key will be encrypted in the new draft."
              : row?.hasKey
                ? "Leave blank to keep the saved credential."
                : "ERPX never returns the key after it is saved.";

  return (
    <form
      className={styles.form}
      aria-busy={pending}
      onSubmit={async (event) => {
        event.preventDefault();
        if (invalid || needsKeyForChangedUrl || needsCredentialStorage) return;
        await save({
          expectedVersion: row?.version ?? 0,
          name,
          baseUrl,
          authMode,
          apiKey,
          removeKey,
          providerPreset: preset,
          textModel,
          imageModel,
        });
      }}
    >
      <fieldset className={styles.fieldset} disabled={pending}>
        <section className={styles.section} aria-labelledby={`${formId}-access`}>
          <div className={styles.sectionHeader}>
            <div>
              <span className={styles.eyebrow}>CONNECTION</span>
              <h3 id={`${formId}-access`}>Provider and credentials</h3>
              <p>Choose where ERPX sends requests and how it authenticates.</p>
            </div>
            <span
              className={`${styles.securityNote} ${authMode === "bearer" && !keyStorageReady ? styles.securityWarning : ""}`}
            >
              {authMode === "none" ? (
                <Info size={15} aria-hidden="true" />
              ) : keyStorageReady ? (
                <ShieldCheck size={15} aria-hidden="true" />
              ) : (
                <AlertTriangle size={15} aria-hidden="true" />
              )}
              {authMode === "none"
                ? "No credential stored"
                : keyStorageReady
                  ? "Encrypted when saved"
                  : "Credential storage unavailable"}
            </span>
          </div>

          <div className={styles.grid}>
            <label className={styles.field}>
              <span>Provider</span>
              <select
                data-autofocus={edit === "new" ? "" : undefined}
                value={preset}
                onChange={(event) =>
                  choosePreset(event.target.value as AiPreset)
                }
              >
                {(Object.keys(aiPresets) as AiPreset[]).map((provider) => (
                  <option key={provider} value={provider}>
                    {aiPresets[provider].name}
                  </option>
                ))}
              </select>
            </label>
            <label className={styles.field}>
              <span>Connection name</span>
              <input
                data-autofocus={edit !== "new" ? "" : undefined}
                required
                maxLength={100}
                value={name}
                onChange={(event) => setName(event.target.value)}
              />
            </label>
            <label className={`${styles.field} ${styles.wideField}`}>
              <span>Base URL</span>
              <input
                type="url"
                required
                value={baseUrl}
                placeholder="https://provider.com/v1"
                onChange={(event) => {
                  invalidate();
                  setBaseUrl(event.target.value);
                  setPreset(presetForUrl(event.target.value));
                }}
              />
            </label>
            <label className={styles.field}>
              <span>Authentication</span>
              <select
                value={authMode}
                onChange={(event) => {
                  invalidate();
                  const next = event.target.value as "bearer" | "none";
                  setAuthMode(next);
                  setApiKey("");
                  setRemoveKey(next === "none" && !!row?.hasKey);
                }}
              >
                <option value="bearer">API key</option>
                <option value="none">No authentication</option>
              </select>
            </label>
            <div className={styles.field}>
              <label htmlFor={apiKeyId}>API key</label>
              <input
                id={apiKeyId}
                type="password"
                autoComplete="new-password"
                value={apiKey}
                disabled={authMode === "none" || removeKey}
                aria-describedby={apiKeyHelpId}
                aria-invalid={
                  needsKeyForChangedUrl || needsCredentialStorage || undefined
                }
                placeholder={apiKeyPlaceholder}
                onChange={(event) => {
                  invalidate();
                  setApiKey(event.target.value);
                  if (event.target.value) setRemoveKey(false);
                }}
              />
              <small
                id={apiKeyHelpId}
                className={
                  needsKeyForChangedUrl || needsCredentialStorage
                    ? styles.fieldError
                    : undefined
                }
              >
                {apiKeyHelp}
              </small>
            </div>
          </div>

          {row?.hasKey && authMode === "bearer" && (
            <label
              className={`${styles.removeKey} ${removeKey ? styles.removeKeyActive : ""}`}
            >
              <input
                type="checkbox"
                checked={removeKey}
                onChange={(event) => {
                  invalidate();
                  setRemoveKey(event.target.checked);
                  if (event.target.checked) setApiKey("");
                }}
              />
              <span>
                <strong>Remove saved key</strong>
                <small>
                  The active revision keeps working until you activate this
                  draft.
                </small>
              </span>
            </label>
          )}

          {presetForUrl(baseUrl) === "opencode-go" && (
            <aside className={styles.providerNotice}>
              <Info size={18} aria-hidden="true" />
              <div>
                <strong>OpenCode Go is optimized for coding agents.</strong>
                <p>
                  Check that your plan supports the ERP workloads you intend to
                  run.
                </p>
                <a
                  href="https://opencode.ai/docs/go/"
                  target="_blank"
                  rel="noreferrer"
                >
                  Review Go documentation
                  <ExternalLink size={13} aria-hidden="true" />
                </a>
              </div>
            </aside>
          )}
        </section>

        <section className={styles.section} aria-labelledby={`${formId}-models`}>
          <div className={styles.sectionHeader}>
            <div>
              <span className={styles.eyebrow}>MODEL CATALOG</span>
              <h3 id={`${formId}-models`}>Models and document reading</h3>
              <p>Refresh the provider catalog, then choose what ERPX should use.</p>
            </div>
            <button
              type="button"
              className={`button secondary ${styles.catalogButton}`}
              disabled={loading || !baseUrl}
              onClick={() => void discover()}
            >
              <RefreshCw
                size={15}
                aria-hidden="true"
                className={loading ? styles.spinning : undefined}
              />
              {loading
                ? "Fetching models…"
                : fetchedAt
                  ? "Refresh models"
                  : "Fetch models"}
            </button>
          </div>

          {discoverError && (
            <div role="alert" className={styles.errorNotice}>
              <Info size={17} aria-hidden="true" />
              <span>
                <strong>{discoverError}</strong>
                You can still enter a supported model ID manually.
              </span>
            </div>
          )}

          {fetchedAt && (
            <div role="status" className={styles.catalogStatus}>
              <CheckCircle2 size={18} aria-hidden="true" />
              <div>
                <strong>
                  {selectableModels.length} supported model
                  {selectableModels.length === 1 ? "" : "s"}
                </strong>
                <span>
                  {totalModels} fetched · Updated{" "}
                  <TimeText value={fetchedAt} />
                </span>
                <small>
                  Catalog availability does not verify the API key or image
                  support.
                  {unsupportedCount
                    ? ` ${unsupportedCount} ${unsupportedCount === 1 ? "model uses" : "models use"} a connection method ERPX does not support yet.`
                    : ""}
                </small>
              </div>
            </div>
          )}

          <div className={styles.modelGrid}>
            <label className={styles.field}>
              <span>Text model ID</span>
              <input
                required
                maxLength={200}
                list={modelListId}
                value={textModel}
                placeholder="Search models or enter an ID"
                onChange={(event) => chooseModel("text", event.target.value)}
              />
              {unsupported(textModel) && (
                <small role="alert" className={styles.fieldError}>
                  {unsupportedModelMessage}
                </small>
              )}
            </label>

            <label className={styles.documentOption}>
              <input
                type="checkbox"
                checked={sameModel}
                onChange={(event) => {
                  setSameModel(event.target.checked);
                  setImageModel(event.target.checked ? textModel : "");
                }}
              />
              <span>
                <strong>Read documents with the main model</strong>
                <small>
                  Receipt reading turns on after this model passes an image test.
                </small>
              </span>
            </label>

            {!sameModel && (
              <label className={`${styles.field} ${styles.wideField}`}>
                <span>Image model ID</span>
                <input
                  maxLength={200}
                  list={modelListId}
                  value={imageModel}
                  placeholder="Optional; may match the text model"
                  onChange={(event) =>
                    chooseModel("image", event.target.value)
                  }
                />
                {unsupported(imageModel) && (
                  <small role="alert" className={styles.fieldError}>
                    {unsupportedModelMessage}
                  </small>
                )}
              </label>
            )}
          </div>
          <datalist id={modelListId}>
            {selectableModels.map((id) => (
              <option key={id} value={id} />
            ))}
          </datalist>
        </section>

        {error && (
          <div role="alert" className={styles.errorNotice}>
            <Info size={17} aria-hidden="true" />
            <strong>{error}</strong>
          </div>
        )}

        <footer className={styles.footer}>
          <div className={styles.nextStep}>
            <Sparkles size={17} aria-hidden="true" />
            <span>
              <strong>Next: test and activate</strong>
              <small>
                Saving creates a draft. The active connection does not change
                until activation.
              </small>
            </span>
          </div>
          <div className={styles.footerActions}>
            <button
              type="button"
              className="button secondary"
              onClick={onCancel}
            >
              Cancel
            </button>
            <button
              className="button"
              disabled={
                loading ||
                invalid ||
                needsKeyForChangedUrl ||
                needsCredentialStorage
              }
            >
              {pending ? "Saving…" : "Save connection"}
            </button>
          </div>
        </footer>
      </fieldset>
    </form>
  );
}
