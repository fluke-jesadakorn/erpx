'use client';

import { useState, useTransition } from 'react';
import { useRouter } from 'next/navigation';
import { Server } from 'lucide-react';
import { T } from '@/components/i18n/T';
import { Alert, Badge, Empty, Modal, Panel, Status, useToast, type BadgeTone } from '@/components/ui';

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

interface ProviderHealth {
  provider_id: number;
  ok: boolean;
  model_count: number | null;
  latency_ms: number | null;
  error: string | null;
  checked_at: string;
}

interface EditState {
  name: string;
  base_url: string;
  enabled: boolean;
  notes: string;
  api_key: string;
}

interface Draft {
  name: string;
  type: ProviderRow['type'];
  base_url: string;
  api_key: string;
  notes: string;
}

interface Props {
  initialProviders: ProviderRow[];
  health: ProviderHealth[];
  canEdit: boolean;
  canCreate: boolean;
  canDelete: boolean;
  canTest: boolean;
}

const EMPTY: Draft = { name: '', type: 'openai_compat', base_url: '', api_key: '', notes: '' };
const INPUT = 'h-9 w-full rounded-md border border-rule bg-paper px-2 text-xs text-ink focus:border-accent';

function editState(provider: ProviderRow): EditState {
  return { name: provider.name, base_url: provider.base_url, enabled: provider.enabled, notes: provider.notes ?? '', api_key: '' };
}

function dirty(provider: ProviderRow, edit: EditState) {
  return provider.name !== edit.name || provider.base_url !== edit.base_url || provider.enabled !== edit.enabled || (provider.notes ?? '') !== edit.notes || edit.api_key.trim().length > 0;
}

function healthMeta(health?: ProviderHealth): { label: string; tone: BadgeTone } {
  if (!health) return { label: 'Not tested', tone: 'neutral' };
  if (health.ok) return { label: `Reachable · ${health.model_count ?? 0} models · ${health.latency_ms ?? 0}ms`, tone: 'positive' };
  return { label: `Unreachable · ${(health.error || 'unknown').slice(0, 40)}`, tone: 'critical' };
}

export function ProvidersTab({ initialProviders, health, canEdit, canCreate, canDelete, canTest }: Props) {
  const router = useRouter();
  const toast = useToast();
  const healthById = new Map(health.map((item) => [item.provider_id, item]));
  const [edits, setEdits] = useState<Record<number, EditState>>(() => Object.fromEntries(initialProviders.map((provider) => [provider.id, editState(provider)])));
  const [saving, setSaving] = useState<number | null>(null);
  const [deleting, setDeleting] = useState<number | null>(null);
  const [deletingId, setDeletingId] = useState<number | null>(null);
  const [testing, setTesting] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [draft, setDraft] = useState<Draft>(EMPTY);
  const [creating, setCreating] = useState(false);
  const [, startTransition] = useTransition();

  const update = (id: number, patch: Partial<EditState>) => {
    const provider = initialProviders.find((item) => item.id === id);
    if (!provider) return;
    setEdits((current) => ({ ...current, [id]: { ...(current[id] ?? editState(provider)), ...patch } }));
    setError(null);
    setInfo(null);
  };

  const reset = (id: number) => {
    const provider = initialProviders.find((item) => item.id === id);
    if (provider) setEdits((current) => ({ ...current, [id]: editState(provider) }));
    setError(null);
    setInfo(null);
  };

  const save = async (id: number) => {
    if (!canEdit || !edits[id]) return;
    const edit = edits[id];
    setSaving(id);
    setError(null);
    setInfo(null);
    try {
      const body: Record<string, unknown> = { name: edit.name, base_url: edit.base_url, enabled: edit.enabled, notes: edit.notes };
      if (edit.api_key.trim()) body.api_key = edit.api_key.trim();
      const { updateProvider } = await import('@/app/actions/ai');
      await updateProvider(id, body);
      const msg = `Provider #${id} saved.`;
      setInfo(msg);
      toast.success(msg);
      setEdits((current) => ({ ...current, [id]: { ...current[id], api_key: '' } }));
      startTransition(() => router.refresh());
    } catch (cause) {
      const msg = cause instanceof Error ? cause.message : 'Save failed';
      setError(msg);
      toast.error(msg);
    } finally {
      setSaving(null);
    }
  };

  const remove = async (id: number) => {
    setDeleting(id);
    setDeletingId(id);
    setError(null);
    setInfo(null);
    try {
      const { deleteProvider } = await import('@/app/actions/ai');
      await deleteProvider(id);
      const msg = `Provider #${id} deleted.`;
      setInfo(msg);
      toast.success(msg);
      startTransition(() => router.refresh());
    } catch (cause) {
      const msg = cause instanceof Error ? cause.message : 'Delete failed';
      setError(msg);
      toast.error(msg);
    } finally {
      setDeleting(null);
      setDeletingId(null);
    }
  };

  const test = async (id: number) => {
    if (!canTest) return;
    setTesting(id);
    setError(null);
    setInfo(null);
    try {
      const { testProvider } = await import('@/app/actions/ai');
      const result = await testProvider(id);
      const msg = `#${id}: ${result.modelCount} model(s) visible at ${result.baseUrl} (${result.latencyMs}ms)`;
      setInfo(msg);
      toast.info(msg);
    } catch (cause) {
      const msg = cause instanceof Error ? cause.message : 'Test failed';
      setError(msg);
      toast.error(msg);
    } finally {
      setTesting(null);
    }
  };

  const create = async () => {
    if (!canCreate || !draft.name || !draft.base_url) {
      setError('name and base_url are required for new provider.');
      return;
    }
    setCreating(true);
    setError(null);
    setInfo(null);
    try {
      const { createProvider } = await import('@/app/actions/ai');
      await createProvider(draft);
      const msg = `Provider '${draft.name}' created.`;
      setInfo(msg);
      toast.success(msg);
      setDraft(EMPTY);
      setShowCreate(false);
      startTransition(() => router.refresh());
    } catch (cause) {
      const msg = cause instanceof Error ? cause.message : 'Create failed';
      setError(msg);
      toast.error(msg);
    } finally {
      setCreating(false);
    }
  };

  return (
    <Panel padding="none" className="overflow-hidden">
      <div className="space-y-3 border-b border-rule p-4">
        {error && <Alert tone="critical" title="Provider update failed">{error}</Alert>}
        {info && <Alert tone="positive" title={info} />}
        {canCreate && <div className="flex justify-end"><button type="button" className="h-9 rounded-md bg-accent px-3 text-xs font-medium text-accent-ink hover:bg-accent-strong disabled:opacity-40" onClick={() => setShowCreate((value) => !value)} disabled={creating}>{showCreate ? <T id="common.cancel" /> : <T id="aiSettings.newProvider" />}</button></div>}
        {showCreate && (
          <Panel padding="sm" className="bg-paper">
            <div className="mb-3 text-xs font-medium uppercase tracking-wider text-mute"><T id="aiSettings.newProvider" /></div>
            <div className="grid gap-3 sm:grid-cols-2">
              <label className="space-y-1"><span className="text-xs text-mute"><T id="aiSettings.fieldName" /></span><input className={INPUT} value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} disabled={creating} /></label>
              <label className="space-y-1"><span className="text-xs text-mute"><T id="aiSettings.fieldType" /></span><select className={INPUT} value={draft.type} onChange={(event) => setDraft({ ...draft, type: event.target.value as Draft['type'] })} disabled={creating}><option value="openai_compat">openai_compat</option><option value="ollama">ollama</option><option value="minimax">minimax</option></select></label>
              <label className="space-y-1 sm:col-span-2"><span className="text-xs text-mute">base_url</span><input className={[INPUT, 'font-mono'].join(' ')} value={draft.base_url} onChange={(event) => setDraft({ ...draft, base_url: event.target.value })} disabled={creating} /></label>
              <label className="space-y-1 sm:col-span-2"><span className="text-xs text-mute"><T id="aiSettings.fieldApiKey" /></span><input type="password" className={[INPUT, 'font-mono'].join(' ')} value={draft.api_key} onChange={(event) => setDraft({ ...draft, api_key: event.target.value })} disabled={creating} /></label>
              <label className="space-y-1 sm:col-span-2"><span className="text-xs text-mute"><T id="aiSettings.fieldNotes" /></span><input className={INPUT} value={draft.notes} onChange={(event) => setDraft({ ...draft, notes: event.target.value })} disabled={creating} /></label>
            </div>
            <div className="mt-3 flex justify-end gap-2"><button type="button" className="h-9 rounded-md border border-rule bg-paper px-3 text-xs text-ink-2 hover:bg-paper-3" onClick={() => { setShowCreate(false); setDraft(EMPTY); }} disabled={creating}><T id="common.cancel" /></button><button type="button" className="h-9 rounded-md bg-accent px-3 text-xs font-medium text-accent-ink hover:bg-accent-strong disabled:opacity-40" onClick={create} disabled={creating}>{creating ? <T id="aiSettings.creating" /> : <T id="aiSettings.create" />}</button></div>
          </Panel>
        )}
      </div>

      {initialProviders.length === 0 ? (
        <Empty icon={Server} title={<T id="aiSettings.empty" />} />
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="border-b border-rule bg-paper-3 text-xs uppercase tracking-wider text-mute"><tr><th className="px-3 py-3 text-left">#</th><th className="px-3 py-3 text-left"><T id="aiSettings.colName" /></th><th className="px-3 py-3 text-left"><T id="aiSettings.colType" /></th><th className="px-3 py-3 text-left">base_url</th><th className="px-3 py-3 text-left"><T id="aiSettings.colKey" /></th><th className="px-3 py-3 text-left"><T id="aiSettings.colNotes" /></th><th className="px-3 py-3 text-right"><T id="aiSettings.colActions" /></th></tr></thead>
            <tbody>
              {initialProviders.map((provider) => {
                const edit = edits[provider.id] ?? editState(provider);
                const changed = dirty(provider, edit);
                const busy = saving === provider.id || deleting === provider.id;
                const meta = healthMeta(healthById.get(provider.id));
                return (
                  <tr key={provider.id} className="border-t border-rule align-top hover:bg-paper-3">
                    <td className="px-3 py-3 font-mono text-xs tabular-nums text-mute">{provider.id}</td>
                    <td className="min-w-40 px-3 py-3"><input className={INPUT} value={edit.name} onChange={(event) => update(provider.id, { name: event.target.value })} disabled={!canEdit || busy} /><div className="mt-1 flex flex-wrap gap-1">{provider.preset && <Badge tone="accent" size="sm">preset: {provider.preset}</Badge>}<Badge tone={meta.tone} size="sm" title={healthById.get(provider.id)?.error ?? ''}>{meta.label}</Badge></div></td>
                    <td className="px-3 py-3 font-mono text-xs text-ink-2">{provider.type}</td>
                    <td className="min-w-56 px-3 py-3"><input className={[INPUT, 'font-mono'].join(' ')} value={edit.base_url} onChange={(event) => update(provider.id, { base_url: event.target.value })} disabled={!canEdit || busy} /><label className="mt-2 inline-flex items-center gap-2 text-xs text-mute"><input type="checkbox" checked={edit.enabled} onChange={(event) => update(provider.id, { enabled: event.target.checked })} disabled={!canEdit || busy} /><T id="aiSettings.fieldEnabled" /></label></td>
                    <td className="min-w-40 px-3 py-3"><Status tone={edit.api_key ? 'caution' : provider.has_api_key ? 'positive' : 'neutral'} size="sm">{edit.api_key ? <T id="aiSettings.keyPending" /> : provider.has_api_key ? <T id="aiSettings.keyEncrypted" /> : <T id="aiSettings.keyNone" />}</Status><input type="password" className={[INPUT, 'mt-2 font-mono'].join(' ')} value={edit.api_key} placeholder={provider.has_api_key ? 'leave blank to keep' : 'enter key'} onChange={(event) => update(provider.id, { api_key: event.target.value })} disabled={!canEdit || busy} /></td>
                    <td className="min-w-40 px-3 py-3"><input className={INPUT} value={edit.notes} onChange={(event) => update(provider.id, { notes: event.target.value })} disabled={!canEdit || busy} /></td>
                    <td className="px-3 py-3 text-right"><div className="inline-flex flex-col items-end gap-1">{canTest && <button type="button" className="h-8 rounded-md border border-rule bg-paper px-2 text-xs text-ink-2 hover:bg-paper-3 disabled:opacity-40" onClick={() => test(provider.id)} disabled={testing === provider.id || busy}>{testing === provider.id ? <T id="aiSettings.testing" /> : <T id="aiSettings.test" />}</button>}{canEdit && <div className="inline-flex gap-1"><button type="button" className="h-8 rounded-md border border-rule bg-paper px-2 text-xs text-ink-2 hover:bg-paper-3 disabled:opacity-40" onClick={() => reset(provider.id)} disabled={!changed || busy}><T id="common.cancel" /></button><button type="button" className="h-8 rounded-md bg-accent px-2 text-xs text-accent-ink hover:bg-accent-strong disabled:opacity-40" onClick={() => save(provider.id)} disabled={!changed || busy}>{saving === provider.id ? <T id="common.saving" /> : <T id="common.save" />}</button></div>}{canDelete && !provider.preset && <button type="button" className="h-8 rounded-md bg-critical px-2 text-xs text-paper hover:bg-critical-strong disabled:opacity-40" onClick={() => setDeletingId(provider.id)} disabled={busy}>{deleting === provider.id ? <T id="aiSettings.deleting" /> : <T id="common.delete" />}</button>}</div></td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      <Modal
        open={deletingId !== null && !initialProviders.find((p) => p.id === deletingId)?.preset}
        onClose={() => (deleting !== null ? null : setDeletingId(null))}
        title="Delete provider"
        subtitle={`Delete provider #${deletingId}?`}
        tone="rose"
        width="md"
        footer={
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setDeletingId(null)}
              disabled={deleting !== null}
              className="rounded-lg border border-rule px-3 py-1.5 text-xs text-ink-2 hover:bg-paper-2 disabled:opacity-50"
            >
              <T id="common.cancel" />
            </button>
            <button
              type="button"
              onClick={() => deletingId !== null && remove(deletingId)}
              disabled={deleting !== null}
              className="rounded-lg bg-critical px-3 py-1.5 text-xs text-paper hover:bg-critical-strong disabled:opacity-50"
            >
              {deleting !== null ? <T id="aiSettings.deleting" /> : <T id="common.delete" />}
            </button>
          </div>
        }
      >
        <p className="text-sm text-ink-2">
          This will remove the provider, its models and its assignments.
        </p>
      </Modal>
    </Panel>
  );
}
