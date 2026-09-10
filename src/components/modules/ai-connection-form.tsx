"use client";
import { useEffect, useRef, useState } from "react";
import { aiPresets, supportedModelProtocol, supportedModels, unsupportedModelMessage, presetForUrl, type AiPreset } from "@/lib/ai/providers";
import type { AiConnectionView } from "@/server/ai-providers";

export function AiConnectionForm({ edit, base, pending, error, save }: {
  edit: AiConnectionView | "new"; base: string; pending: boolean; error: string;
  save: (input: unknown) => Promise<void>;
}) {
  const row = edit === "new" ? null : edit;
  const [preset, setPreset] = useState<AiPreset>(row?.provider_preset ?? "opencode-zen");
  const [baseUrl, setBaseUrl] = useState(row?.base_url ?? aiPresets["opencode-zen"].baseUrl);
  const [name, setName] = useState(row?.name ?? "OpenCode Zen");
  const [authMode, setAuthMode] = useState(row?.auth_mode ?? "bearer");
  const [apiKey, setApiKey] = useState("");
  const [removeKey, setRemoveKey] = useState(false);
  const [textModel, setTextModel] = useState(row?.text_model ?? "");
  const [sameModel, setSameModel] = useState(!row || (!!row.image_model && row.image_model === row.text_model));
  const [imageModel, setImageModel] = useState(row?.image_model ?? "");
  const [models, setModels] = useState<string[]>([]);
  const [totalModels, setTotalModels] = useState(0);
  const [fetchedAt, setFetchedAt] = useState("");
  const [discoverError, setDiscoverError] = useState("");
  const [loading, setLoading] = useState(false);
  const generation = useRef(0), controller = useRef<AbortController | null>(null);
  useEffect(() => () => { generation.current++; controller.current?.abort(); }, []);
  function invalidate() {
    generation.current++; controller.current?.abort(); setLoading(false);
    setModels([]); setTotalModels(0); setFetchedAt(""); setDiscoverError("");
  }
  function choosePreset(next: AiPreset) {
    invalidate(); setPreset(next); setBaseUrl(aiPresets[next].baseUrl);
    setName(aiPresets[next].name); setApiKey(""); setRemoveKey(false); setAuthMode("bearer");
    setTextModel(""); setImageModel(""); setSameModel(true);
  }
  function chooseModel(kind: "text" | "image", id: string) {
    if (kind === "text") { setTextModel(id); if (sameModel) setImageModel(id); }
    else setImageModel(id);
  }

  async function discover() {
    controller.current?.abort(); const abort = new AbortController(); controller.current = abort;
    const requestId = ++generation.current; setLoading(true); setDiscoverError("");
    try {
      const response = await fetch(`/api/v1${base}/ai-providers/discover`, {
        method: "POST", headers: { "Content-Type": "application/json" }, signal: abort.signal, cache: "no-store",
        body: JSON.stringify({ baseUrl, authMode, apiKey, removeKey, connectionId: row?.id }),
      });
      const result = await response.json() as { data: { models: string[]; totalModels: number; fetchedAt: string }; error?: { message: string } };
      if (!response.ok) throw Error(result.error?.message ?? "Model discovery failed.");
      if (requestId === generation.current) { setModels(result.data.models); setTotalModels(result.data.totalModels); setFetchedAt(result.data.fetchedAt); }
    } catch (e) {
      if (requestId === generation.current && !abort.signal.aborted) setDiscoverError(e instanceof Error ? e.message : "Model discovery failed.");
    } finally { if (requestId === generation.current) setLoading(false); }
  }
  const unsupported = (id: string) => !!id.trim() && !supportedModelProtocol(baseUrl, id);
  const selectableModels = supportedModels(baseUrl, models);
  const invalid = !textModel.trim() || unsupported(textModel) || unsupported(imageModel);
  return <form onSubmit={async e => {
    e.preventDefault();
    if (invalid) return;
    await save({ expectedVersion: row?.version ?? 0, name, baseUrl, authMode, apiKey, removeKey, providerPreset: preset, textModel, imageModel });
  }}>
    <fieldset disabled={pending} style={{ border: 0, padding: 0, margin: 0, minWidth: 0 }}>
      <div className="business-form">
        <label>Provider<select value={preset} onChange={e => choosePreset(e.target.value as AiPreset)}>{(Object.keys(aiPresets) as AiPreset[]).map(p => <option key={p} value={p}>{aiPresets[p].name}</option>)}</select></label>
        <label>Connection name<input required maxLength={100} value={name} onChange={e => setName(e.target.value)} /></label>
        <label>Base URL<input type="url" required value={baseUrl} placeholder="https://provider.com/v1" onChange={e => { invalidate(); setBaseUrl(e.target.value); setPreset(presetForUrl(e.target.value)); }} /></label>
        <label>Authentication<select value={authMode} onChange={e => { invalidate(); setAuthMode(e.target.value as "bearer" | "none"); }}><option value="bearer">API key</option><option value="none">No authentication</option></select></label>
        <label>API key<input type="password" autoComplete="new-password" value={apiKey} disabled={authMode === "none"} placeholder={row?.hasKey ? "Leave blank to keep saved key at the same URL" : ""} onChange={e => { invalidate(); setApiKey(e.target.value); }} /></label>
      </div>
      {row?.hasKey && <label><input type="checkbox" checked={removeKey} onChange={e => { invalidate(); setRemoveKey(e.target.checked); }} /> Remove saved key</label>}
      {presetForUrl(baseUrl) === "opencode-go" && <p>OpenCode Go is designed for coding agents. ERP workloads may not be supported by your subscription. <a href="https://opencode.ai/docs/go/" target="_blank" rel="noreferrer">Go documentation</a></p>}
      <button type="button" className="button secondary" disabled={loading || !baseUrl} onClick={() => void discover()}>{loading ? "Fetching models…" : fetchedAt ? "Refresh models" : "Fetch models"}</button>
      {discoverError && <p role="alert" className="error">{discoverError} You can still enter a supported model ID manually.</p>}
      {fetchedAt && <p role="status">{selectableModels.length} supported / {totalModels} fetched models · Updated {new Date(fetchedAt).toLocaleString()}. Availability does not verify your key or image support.</p>}
      {fetchedAt && totalModels > selectableModels.length && <p role="note">{totalModels - selectableModels.length} models use a connection method ERPX does not support yet.</p>}
      <div className="business-form">
        <label>Text model ID<input required maxLength={200} list="provider-model-ids" value={textModel} placeholder="Search models or enter an ID" onChange={e => chooseModel("text", e.target.value)} />{unsupported(textModel) && <span role="alert" className="error">{unsupportedModelMessage}</span>}</label>
        <label>Read documents with the main model<input type="checkbox" checked={sameModel} onChange={e => { setSameModel(e.target.checked); setImageModel(e.target.checked ? textModel : ""); }} /><span>The model must pass an image test before receipt reading is enabled.</span></label>
        {!sameModel && <label>Image model ID<input maxLength={200} list="provider-model-ids" value={imageModel} placeholder="Optional; may match text model" onChange={e => chooseModel("image", e.target.value)} />{unsupported(imageModel) && <span role="alert" className="error">{unsupportedModelMessage}</span>}</label>}
      </div>
      <datalist id="provider-model-ids">{selectableModels.map(id => <option key={id} value={id} />)}</datalist>
      {fetchedAt && !selectableModels.length && <p role="status">No supported models found for this provider.</p>}
      <p>The connection method is configured automatically for your selected model.</p>
      {error && <p role="alert" className="error">{error}</p>}
      <p>Save, then test and activate this revision. Existing active settings remain in use until activation.</p>
      <button className="button" disabled={loading || invalid}>Save connection</button>
    </fieldset>
  </form>;
}
