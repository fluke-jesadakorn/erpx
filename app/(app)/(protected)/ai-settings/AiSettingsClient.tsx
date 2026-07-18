'use client';

import React, { useState, useTransition } from 'react';
import { useRouter } from 'next/navigation';
import { T } from '@/components/i18n/T';
import { Modal, useToast } from '@/components/ui';

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
  const toast = useToast();
  const [edits, setEdits] = useState<Record<number, EditState>>(() => {
    const m: Record<number, EditState> = {};
    for (const p of initialProviders) m[p.id] = toEditState(p);
    return m;
  });
  const [savingId, setSavingId] = useState<number | null>(null);
  const [deletingId, setDeletingId] = useState<number | null>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<number | null>(null);
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
      const savedMsg = `Provider #${id} saved.`;
      setInfo(savedMsg);
      toast.success(savedMsg);
      setEdits((prev) => {
        const next = { ...prev };
        const cur = next[id];
        if (cur) next[id] = { ...cur, api_key: '' };
        return next;
      });
      startTransition(() => router.refresh());
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Save failed';
      setError(msg);
      toast.error(msg);
    } finally {
      setSavingId(id);
      setSavingId(null);
    }
  };

  const remove = async (id: number) => {
    if (!canDelete) return;
    setDeletingId(id);
    setError(null);
    setInfo(null);
    try {
      const res = await fetch(`/api/ai/providers/${id}`, { method: 'DELETE' });
      const data = await res.json().catch(() => ({}));
      if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`);
      const msg = `Provider #${id} deleted.`;
      setInfo(msg);
      toast.success(msg);
      startTransition(() => router.refresh());
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Delete failed';
      setError(msg);
      toast.error(msg);
    } finally {
      setDeletingId(null);
      setConfirmDeleteId(null);
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
      const msg = `#${id}: ${data.modelCount} model(s) visible at ${data.baseUrl} (${data.latencyMs}ms)`;
      setInfo(msg);
      toast.info(msg);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Test failed';
      setError(msg);
      toast.error(msg);
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
      const msg = `Provider '${draft.name}' created.`;
      setInfo(msg);
      toast.success(msg);
      setDraft(EMPTY_NEW);
      setShowCreate(false);
      startTransition(() => router.refresh());
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Create failed';
      setError(msg);
      toast.error(msg);
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="space-y-4">
      {error ? <div className="text-critical text-xs">{error}</div> : null}
      {info ? <div className="text-positive text-xs">{info}</div> : null}

      <div className="flex justify-end gap-2">
        {canCreate ? (
          <button
            type="button"
            className="px-3 py-1 rounded bg-info-strong hover:bg-info text-xs disabled:opacity-40"
            onClick={() => setShowCreate((v) => !v)}
            disabled={creating}
          >
            {showCreate ? <T id="common.cancel" /> : <T id="aiSettings.newProvider" />}
          </button>
        ) : null}
      </div>

      {showCreate ? (
        <div className="bg-paper-2/60 border border-rule rounded p-4 space-y-3">
          <div className="text-xs uppercase tracking-wider text-ink-2">
            <T id="aiSettings.newProvider" />
          </div>
          <div className="grid grid-cols-2 gap-3">
            <label className="space-y-1">
              <div className="text-xs text-ink-2">
                <T id="aiSettings.fieldName" />
              </div>
              <input
                type="text"
                className="w-full px-2 py-1 rounded bg-paper border border-rule text-xs text-ink"
                value={draft.name}
                onChange={(e) => setDraft({ ...draft, name: e.target.value })}
                disabled={creating}
              />
            </label>
            <label className="space-y-1">
              <div className="text-xs text-ink-2">
                <T id="aiSettings.fieldType" />
              </div>
              <select
                className="w-full px-2 py-1 rounded bg-paper border border-rule text-xs text-ink"
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
              <div className="text-xs text-ink-2">base_url</div>
              <input
                type="text"
                className="w-full px-2 py-1 rounded bg-paper border border-rule text-xs font-mono text-ink"
                value={draft.base_url}
                onChange={(e) => setDraft({ ...draft, base_url: e.target.value })}
                disabled={creating}
                placeholder="https://api.example.com/v1"
              />
            </label>
            <label className="space-y-1 col-span-2">
              <div className="text-xs text-ink-2">
                <T id="aiSettings.fieldApiKey" />
              </div>
              <input
                type="password"
                className="w-full px-2 py-1 rounded bg-paper border border-rule text-xs font-mono text-ink"
                value={draft.api_key}
                onChange={(e) => setDraft({ ...draft, api_key: e.target.value })}
                disabled={creating}
                placeholder="sk-…"
              />
            </label>
            <label className="space-y-1 col-span-2">
              <div className="text-xs text-ink-2">
                <T id="aiSettings.fieldNotes" />
              </div>
              <input
                type="text"
                className="w-full px-2 py-1 rounded bg-paper border border-rule text-xs text-ink"
                value={draft.notes}
                onChange={(e) => setDraft({ ...draft, notes: e.target.value })}
                disabled={creating}
              />
            </label>
          </div>
          <div className="flex gap-2 justify-end">
            <button
              type="button"
              className="px-2 py-1 rounded bg-paper-2 hover:bg-paper-2 text-xs"
              onClick={() => { setShowCreate(false); setDraft(EMPTY_NEW); }}
              disabled={creating}
            >
              <T id="common.cancel" />
            </button>
            <button
              type="button"
              className="px-2 py-1 rounded bg-info-strong hover:bg-info text-xs disabled:opacity-40"
              onClick={submitNew}
              disabled={creating}
            >
              {creating ? <T id="aiSettings.creating" /> : <T id="aiSettings.create" />}
            </button>
          </div>
        </div>
      ) : null}

      <table className="w-full text-sm border-separate border-spacing-y-1">
        <thead>
          <tr className="text-xs uppercase tracking-wider text-ink-2">
            <th className="text-left px-2 py-1">#</th>
            <th className="text-left px-2 py-1">
              <T id="aiSettings.colName" />
            </th>
            <th className="text-left px-2 py-1">
              <T id="aiSettings.colType" />
            </th>
            <th className="text-left px-2 py-1">base_url</th>
            <th className="text-left px-2 py-1">
              <T id="aiSettings.colKey" />
            </th>
            <th className="text-left px-2 py-1">
              <T id="aiSettings.colNotes" />
            </th>
            <th className="text-right px-2 py-1">
              <T id="aiSettings.colActions" />
            </th>
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
              <tr key={p.id} className="bg-paper-2/50 border border-rule align-top">
                <td className="px-2 py-1.5 text-ink-2 text-xs">{p.id}</td>
                <td className="px-2 py-1.5">
                  <input
                    type="text"
                    className="w-full px-2 py-1 rounded bg-paper border border-rule text-xs text-ink"
                    value={e.name}
                    onChange={(ev) => updateEdit(p.id, { name: ev.target.value })}
                    disabled={!canEdit || saving || deleting}
                  />
                  {isPreset ? <div className="mt-1 text-[10px] text-mute">preset: {p.preset}</div> : null}
                </td>
                <td className="px-2 py-1.5 text-ink-2 text-xs font-mono">{p.type}</td>
                <td className="px-2 py-1.5">
                  <input
                    type="text"
                    className="w-full px-2 py-1 rounded bg-paper border border-rule text-xs font-mono text-ink"
                    value={e.base_url}
                    onChange={(ev) => updateEdit(p.id, { base_url: ev.target.value })}
                    disabled={!canEdit || saving || deleting}
                  />
                  <label className="mt-1 inline-flex items-center gap-1 text-[10px] text-ink-2">
                    <input
                      type="checkbox"
                      checked={e.enabled}
                      onChange={(ev) => updateEdit(p.id, { enabled: ev.target.checked })}
                      disabled={!canEdit || saving || deleting}
                    />
                    <T id="aiSettings.fieldEnabled" />
                  </label>
                </td>
                <td className="px-2 py-1.5">
                  <div className="text-[10px] text-ink-2 mb-1">
                    {e.api_key ? <T id="aiSettings.keyPending" /> : p.has_api_key ? <T id="aiSettings.keyEncrypted" /> : <T id="aiSettings.keyNone" />}
                  </div>
                  <input
                    type="password"
                    className="w-full px-2 py-1 rounded bg-paper border border-rule text-xs font-mono text-ink"
                    value={e.api_key}
                    placeholder={p.has_api_key ? 'leave blank to keep current' : 'enter key'}
                    onChange={(ev) => updateEdit(p.id, { api_key: ev.target.value })}
                    disabled={!canEdit || saving || deleting}
                  />
                </td>
                <td className="px-2 py-1.5">
                  <input
                    type="text"
                    className="w-full px-2 py-1 rounded bg-paper border border-rule text-xs text-ink"
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
                          className="px-2 py-1 rounded bg-paper-2 hover:bg-paper-2 text-xs disabled:opacity-40"
                          onClick={() => test(p.id)}
                          disabled={testing || saving || deleting}
                        >
                          {testing ? <T id="aiSettings.testing" /> : <T id="aiSettings.test" />}
                        </button>
                      ) : null}
                      {canEdit ? (
                        <div className="inline-flex gap-1">
                          <button
                            type="button"
                            className="px-2 py-1 rounded bg-paper-2 hover:bg-paper-2 text-xs disabled:opacity-40"
                            onClick={() => resetRow(p.id)}
                            disabled={!dirty || saving || deleting}
                          >
                            <T id="common.cancel" />
                          </button>
                          <button
                            type="button"
                            className="px-2 py-1 rounded bg-info-strong hover:bg-info text-xs disabled:opacity-40"
                            onClick={() => save(p.id)}
                            disabled={!dirty || saving || deleting}
                          >
                            {saving ? <T id="common.saving" /> : <T id="common.save" />}
                          </button>
                        </div>
                      ) : null}
                      {canDelete && !isPreset ? (
                        <button
                          type="button"
                          className="px-2 py-1 rounded bg-critical-strong hover:bg-critical-strong text-xs disabled:opacity-40"
                          onClick={() => setConfirmDeleteId(p.id)}
                          disabled={deleting || saving}
                        >
                          {deleting ? <T id="aiSettings.deleting" /> : <T id="common.delete" />}
                        </button>
                      ) : null}
                    </div>
                  ) : null}
                </td>
              </tr>
            );
          })}
          {initialProviders.length === 0 ? (
            <tr><td colSpan={7} className="text-center text-mute py-4 text-xs">
              <T id="aiSettings.empty" />
            </td></tr>
          ) : null}
        </tbody>
      </table>

      <details className="text-xs text-ink-2">
        <summary className="cursor-pointer">
          <T id="aiSettings.helpTitle" />
        </summary>
        <ul className="mt-1 space-y-0.5 pl-4 list-disc">
          <li>
            <T
              id="aiSettings.helpEncrypt"
              values={{}}
            />
          </li>
          <li>
            <T
              id="aiSettings.helpEnvFallback"
              values={{}}
            />
          </li>
          <li>
            <T
              id="aiSettings.helpTest"
              values={{}}
            />
          </li>
          <li>
            <T
              id="aiSettings.helpGating"
              values={{}}
            />
          </li>
        </ul>
      </details>

      <Modal
        open={confirmDeleteId !== null}
        onClose={() => (deletingId !== null ? null : setConfirmDeleteId(null))}
        title="Delete provider"
        subtitle={`Delete provider #${confirmDeleteId}?`}
        tone="rose"
        width="md"
        footer={
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setConfirmDeleteId(null)}
              disabled={deletingId !== null}
              className="px-3 py-1.5 rounded text-xs text-ink-2 border border-rule hover:bg-paper-2 disabled:opacity-50"
            >
              <T id="common.cancel" />
            </button>
            <button
              type="button"
              onClick={() => confirmDeleteId !== null && remove(confirmDeleteId)}
              disabled={deletingId !== null}
              className="px-3 py-1.5 rounded text-xs bg-critical-strong text-paper hover:bg-critical disabled:opacity-50"
            >
              {deletingId !== null ? <T id="aiSettings.deleting" /> : <T id="common.delete" />}
            </button>
          </div>
        }
      >
        <p className="text-xs text-ink-2 leading-relaxed">
          Models, staff and assignments referencing this provider will lose their foreign key.
        </p>
      </Modal>
    </div>
  );
};