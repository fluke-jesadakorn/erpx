'use client';

import React, { useState, useTransition } from 'react';
import { useRouter } from 'next/navigation';

interface ProviderRow {
  id: number;
  name: string;
  type: 'ollama' | 'openai_compat' | 'minimax';
  base_url: string;
  enabled: boolean;
  preset: string | null;
  notes: string | null;
  has_api_key: boolean;
  created_at: string;
  updated_at: string;
}

interface EditState {
  name: string;
  base_url: string;
  enabled: boolean;
  notes: string;
  api_key: string;
}

interface NewDraft {
  name: string;
  type: 'ollama' | 'openai_compat' | 'minimax';
  base_url: string;
  api_key: string;
  notes: string;
}

const EMPTY_NEW: NewDraft = { name: '', type: 'openai_compat', base_url: '', api_key: '', notes: '' };

interface Props {
  initialProviders: ProviderRow[];
  canEdit: boolean;
  canCreate: boolean;
  canDelete: boolean;
  canTest: boolean;
}

function isDirty(initial: ProviderRow, edit: EditState): boolean {
  if (initial.name !== edit.name) return true;
  if (initial.base_url !== edit.base_url) return true;
  if (initial.enabled !== edit.enabled) return true;
  if ((initial.notes ?? '') !== edit.notes) return true;
  if (edit.api_key.trim().length > 0) return true;
  return false;
}

function toEditState(p: ProviderRow): EditState {
  return {
    name: p.name,
    base_url: p.base_url,
    enabled: p.enabled,
    notes: p.notes ?? '',
    api_key: '',
  };
}

export const AiSettingsClient: React.FC<Props> = ({
  initialProviders,
  canEdit,
  canCreate,
  canDelete,
  canTest,
}) => {
  const router = useRouter();
  const [edits, setEdits] = useState<Record<number, EditState>>(() => {
    const m: Record<number, EditState> = {};
    for (const p of initialProviders) m[p.id] = toEditState(p);
    return m;
  });
  const [savingId, setSavingId] = useState<number | null>(null);
  const [deletingId, setDeletingId] = useState<number | null>(null);
  const [testingId, setTestingId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [draft, setDraft] = useState<NewDraft>(EMPTY_NEW);
  const [creating, setCreating] = useState(false);
  const [, startTransition] = useTransition();

  const updateEdit = (id: number, patch: Partial<EditState>) => {
    setEdits((prev) => ({ ...prev, [id]: { ...(prev[id] || toEditState(initialProviders.find((x) => x.id === id)!)), ...patch } }));
    setError(null);
    setInfo(null);
  };

  const resetRow = (id: number) => {
    const p = initialProviders.find((x) => x.id === id);
    if (!p) return;
    setEdits((prev) => ({ ...prev, [id]: toEditState(p) }));
    setError(null);
    setInfo(null);
  };

  const save = async (id: number) => {
    if (!canEdit) return;
    const e = edits[id];
    if (!e) return;
    setSavingId(id);
    setError(null);
    setInfo(null);
    try {
      const body: Record<string, unknown> = {
        name: e.name,
        base_url: e.base_url,
        enabled: e.enabled,
        notes: e.notes,
      };
      if (e.api_key.trim().length > 0) body.api_key = e.api_key.trim();
      const res = await fetch(`/api/ai/providers/${id}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      const data = await res.json().catch(() => ({}));
      if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`);
      setInfo(`Provider #${id} saved.`);
      setEdits((prev) => {
        const next = { ...prev };
        const cur = next[id];
        if (cur) next[id] = { ...cur, api_key: '' };
        return next;
      });
      startTransition(() => router.refresh());
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Save failed');
    } finally {
      setSavingId(id);
      setSavingId(null);
    }
  };

  const remove = async (id: number) => {
    if (!canDelete) return;
    if (!confirm(`Delete provider #${id}? Models, staff and assignments referencing it will lose their FK.`)) return;
    setDeletingId(id);
    setError(null);
    setInfo(null);
    try {
      const res = await fetch(`/api/ai/providers/${id}`, { method: 'DELETE' });
      const data = await res.json().catch(() => ({}));
      if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`);
      setInfo(`Provider #${id} deleted.`);
      startTransition(() => router.refresh());
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Delete failed');
    } finally {
      setDeletingId(null);
    }
  };

  const test = async (id: number) => {
    if (!canTest) return;
    setTestingId(id);
    setError(null);
    setInfo(null);
    try {
      const res = await fetch('/api/ai/providers/test', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id }),
      });
      const data = await res.json();
      if (!data.ok) throw new Error(data.error || 'Test failed');
      setInfo(`#${id}: ${data.modelCount} model(s) visible at ${data.baseUrl} (${data.latencyMs}ms)`);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Test failed');
    } finally {
      setTestingId(null);
    }
  };

  const submitNew = async () => {
    if (!canCreate) return;
    if (!draft.name || !draft.base_url) {
      setError('name and base_url are required for new provider.');
      return;
    }
    setCreating(true);
    setError(null);
    setInfo(null);
    try {
      const res = await fetch('/api/ai/providers', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(draft),
      });
      const data = await res.json().catch(() => ({}));
      if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`);
      setInfo(`Provider '${draft.name}' created.`);
      setDraft(EMPTY_NEW);
      setShowCreate(false);
      startTransition(() => router.refresh());
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Create failed');
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="space-y-4">
      {error ? <div className="text-rose-300 text-xs">{error}</div> : null}
      {info ? <div className="text-emerald-300 text-xs">{info}</div> : null}

      <div className="flex justify-end gap-2">
        {canCreate ? (
          <button
            type="button"
            className="px-3 py-1 rounded bg-cyan-600 hover:bg-cyan-500 text-xs disabled:opacity-40"
            onClick={() => setShowCreate((v) => !v)}
            disabled={creating}
          >
            {showCreate ? 'Cancel' : '+ New provider'}
          </button>
        ) : null}
      </div>

      {showCreate ? (
        <div className="bg-slate-900/60 border border-slate-700 rounded p-4 space-y-3">
          <div className="text-xs uppercase tracking-wider text-slate-400">New provider</div>
          <div className="grid grid-cols-2 gap-3">
            <label className="space-y-1">
              <div className="text-xs text-slate-400">Name</div>
              <input
                type="text"
                className="w-full px-2 py-1 rounded bg-slate-950 border border-slate-700 text-xs text-slate-100"
                value={draft.name}
                onChange={(e) => setDraft({ ...draft, name: e.target.value })}
                disabled={creating}
              />
            </label>
            <label className="space-y-1">
              <div className="text-xs text-slate-400">Type</div>
              <select
                className="w-full px-2 py-1 rounded bg-slate-950 border border-slate-700 text-xs text-slate-100"
                value={draft.type}
                onChange={(e) => setDraft({ ...draft, type: e.target.value as NewDraft['type'] })}
                disabled={creating}
              >
                <option value="openai_compat">openai_compat</option>
                <option value="ollama">ollama</option>
                <option value="minimax">minimax</option>
              </select>
            </label>
            <label className="space-y-1 col-span-2">
              <div className="text-xs text-slate-400">base_url</div>
              <input
                type="text"
                className="w-full px-2 py-1 rounded bg-slate-950 border border-slate-700 text-xs font-mono text-slate-100"
                value={draft.base_url}
                onChange={(e) => setDraft({ ...draft, base_url: e.target.value })}
                disabled={creating}
                placeholder="https://api.example.com/v1"
              />
            </label>
            <label className="space-y-1 col-span-2">
              <div className="text-xs text-slate-400">api_key (optional — leave blank for keyless providers like local ollama)</div>
              <input
                type="password"
                className="w-full px-2 py-1 rounded bg-slate-950 border border-slate-700 text-xs font-mono text-slate-100"
                value={draft.api_key}
                onChange={(e) => setDraft({ ...draft, api_key: e.target.value })}
                disabled={creating}
                placeholder="sk-…"
              />
            </label>
            <label className="space-y-1 col-span-2">
              <div className="text-xs text-slate-400">notes</div>
              <input
                type="text"
                className="w-full px-2 py-1 rounded bg-slate-950 border border-slate-700 text-xs text-slate-100"
                value={draft.notes}
                onChange={(e) => setDraft({ ...draft, notes: e.target.value })}
                disabled={creating}
              />
            </label>
          </div>
          <div className="flex gap-2 justify-end">
            <button
              type="button"
              className="px-2 py-1 rounded bg-slate-700 hover:bg-slate-600 text-xs"
              onClick={() => { setShowCreate(false); setDraft(EMPTY_NEW); }}
              disabled={creating}
            >
              Cancel
            </button>
            <button
              type="button"
              className="px-2 py-1 rounded bg-cyan-600 hover:bg-cyan-500 text-xs disabled:opacity-40"
              onClick={submitNew}
              disabled={creating}
            >
              {creating ? 'Creating…' : 'Create'}
            </button>
          </div>
        </div>
      ) : null}

      <table className="w-full text-sm border-separate border-spacing-y-1">
        <thead>
          <tr className="text-xs uppercase tracking-wider text-slate-400">
            <th className="text-left px-2 py-1">#</th>
            <th className="text-left px-2 py-1">Name</th>
            <th className="text-left px-2 py-1">Type</th>
            <th className="text-left px-2 py-1">base_url</th>
            <th className="text-left px-2 py-1">Key</th>
            <th className="text-left px-2 py-1">Notes</th>
            <th className="text-right px-2 py-1">Actions</th>
          </tr>
        </thead>
        <tbody>
          {initialProviders.map((p) => {
            const e = edits[p.id] || toEditState(p);
            const dirty = isDirty(p, e);
            const saving = savingId === p.id;
            const deleting = deletingId === p.id;
            const testing = testingId === p.id;
            const isPreset = !!p.preset;
            return (
              <tr key={p.id} className="bg-slate-900/50 border border-slate-700 align-top">
                <td className="px-2 py-1.5 text-slate-400 text-xs">{p.id}</td>
                <td className="px-2 py-1.5">
                  <input
                    type="text"
                    className="w-full px-2 py-1 rounded bg-slate-950 border border-slate-700 text-xs text-slate-100"
                    value={e.name}
                    onChange={(ev) => updateEdit(p.id, { name: ev.target.value })}
                    disabled={!canEdit || saving || deleting}
                  />
                  {isPreset ? <div className="mt-1 text-[10px] text-slate-500">preset: {p.preset}</div> : null}
                </td>
                <td className="px-2 py-1.5 text-slate-300 text-xs font-mono">{p.type}</td>
                <td className="px-2 py-1.5">
                  <input
                    type="text"
                    className="w-full px-2 py-1 rounded bg-slate-950 border border-slate-700 text-xs font-mono text-slate-100"
                    value={e.base_url}
                    onChange={(ev) => updateEdit(p.id, { base_url: ev.target.value })}
                    disabled={!canEdit || saving || deleting}
                  />
                  <label className="mt-1 inline-flex items-center gap-1 text-[10px] text-slate-400">
                    <input
                      type="checkbox"
                      checked={e.enabled}
                      onChange={(ev) => updateEdit(p.id, { enabled: ev.target.checked })}
                      disabled={!canEdit || saving || deleting}
                    />
                    enabled
                  </label>
                </td>
                <td className="px-2 py-1.5">
                  <div className="text-[10px] text-slate-400 mb-1">
                    {e.api_key ? '••• new key pending' : p.has_api_key ? '••• encrypted' : '— none —'}
                  </div>
                  <input
                    type="password"
                    className="w-full px-2 py-1 rounded bg-slate-950 border border-slate-700 text-xs font-mono text-slate-100"
                    value={e.api_key}
                    placeholder={p.has_api_key ? 'leave blank to keep current' : 'enter key'}
                    onChange={(ev) => updateEdit(p.id, { api_key: ev.target.value })}
                    disabled={!canEdit || saving || deleting}
                  />
                </td>
                <td className="px-2 py-1.5">
                  <input
                    type="text"
                    className="w-full px-2 py-1 rounded bg-slate-950 border border-slate-700 text-xs text-slate-100"
                    value={e.notes}
                    onChange={(ev) => updateEdit(p.id, { notes: ev.target.value })}
                    disabled={!canEdit || saving || deleting}
                  />
                </td>
                <td className="px-2 py-1.5 text-right">
                  {canEdit || canDelete || canTest ? (
                    <div className="inline-flex flex-col gap-1 items-end">
                      {canTest ? (
                        <button
                          type="button"
                          className="px-2 py-1 rounded bg-slate-700 hover:bg-slate-600 text-xs disabled:opacity-40"
                          onClick={() => test(p.id)}
                          disabled={testing || saving || deleting}
                        >
                          {testing ? 'Testing…' : 'Test'}
                        </button>
                      ) : null}
                      {canEdit ? (
                        <div className="inline-flex gap-1">
                          <button
                            type="button"
                            className="px-2 py-1 rounded bg-slate-700 hover:bg-slate-600 text-xs disabled:opacity-40"
                            onClick={() => resetRow(p.id)}
                            disabled={!dirty || saving || deleting}
                          >
                            Reset
                          </button>
                          <button
                            type="button"
                            className="px-2 py-1 rounded bg-cyan-600 hover:bg-cyan-500 text-xs disabled:opacity-40"
                            onClick={() => save(p.id)}
                            disabled={!dirty || saving || deleting}
                          >
                            {saving ? 'Saving…' : 'Save'}
                          </button>
                        </div>
                      ) : null}
                      {canDelete && !isPreset ? (
                        <button
                          type="button"
                          className="px-2 py-1 rounded bg-rose-700 hover:bg-rose-600 text-xs disabled:opacity-40"
                          onClick={() => remove(p.id)}
                          disabled={deleting || saving}
                        >
                          {deleting ? 'Deleting…' : 'Delete'}
                        </button>
                      ) : null}
                    </div>
                  ) : null}
                </td>
              </tr>
            );
          })}
          {initialProviders.length === 0 ? (
            <tr><td colSpan={7} className="text-center text-slate-500 py-4 text-xs">No providers registered.</td></tr>
          ) : null}
        </tbody>
      </table>

      <details className="text-xs text-slate-400">
        <summary className="cursor-pointer">What this page does</summary>
        <ul className="mt-1 space-y-0.5 pl-4 list-disc">
          <li>Edits rows in <code>ai_providers</code>. Keys are encrypted at rest via pgcrypto <code>ai_encrypt(plaintext, ENCRYPTION_KEY)</code> and decrypted only at call time by the AI router.</li>
          <li>Env fallback (<code>app/.env.local</code>: <code>MINIMAX_API_KEY</code>, <code>MINIMAX_BASE_URL</code>, <code>MINIMAX_MODEL</code>) is used only when no DB row takes priority for a section/task.</li>
          <li>&ldquo;Test&rdquo; calls <code>/api/ai/providers/test</code> which lists models via the provider&rsquo;s own API (decrypts the key).</li>
          <li>Gated by <code>ai:provider:read</code> / <code>ai:provider:update</code> / <code>ai:provider:create</code> / <code>ai:provider:delete</code> / <code>ai:provider:test</code>.</li>
        </ul>
      </details>
    </div>
  );
};