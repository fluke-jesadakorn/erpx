'use client';

import { useState, useTransition } from 'react';
import { useRouter } from 'next/navigation';
import { T } from '@/components/i18n/T';
import { Alert, Badge, Empty, Panel, Status } from '@/components/ui';

interface AssignmentRow {
  id: number;
  section_key: string;
  task_type: 'embed' | 'chat' | 'vision';
  provider_id: number | null;
  model_id: number | null;
  priority: number;
  enabled: boolean;
  provider_name: string | null;
  model_name: string | null;
}

interface Props {
  initialAssignments: AssignmentRow[];
  providers: { id: number; name: string }[];
  models: { id: number; name: string; capabilities: string[]; provider_id: number }[];
  canCreate: boolean;
  canEdit: boolean;
  canDelete: boolean;
}

interface Draft {
  section_key: string;
  task_type: 'embed' | 'chat' | 'vision';
  provider_id: string;
  model_id: string;
  priority: number;
}

const INPUT = 'h-9 rounded-md border border-rule bg-paper px-2 text-xs text-ink focus:border-accent';

export function AssignmentsTab({ initialAssignments, providers, models, canCreate, canEdit, canDelete }: Props) {
  const router = useRouter();
  const [busyId, setBusyId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [draft, setDraft] = useState<Draft>({ section_key: '', task_type: 'chat', provider_id: providers[0]?.id ? String(providers[0].id) : '', model_id: '', priority: 100 });
  const [, startTransition] = useTransition();

  const update = async (id: number, patch: Record<string, unknown>, message: string) => {
    setBusyId(id);
    setError(null);
    try {
      const { updateAssignment } = await import('@/app/actions/ai');
      await updateAssignment(id, patch);
      setInfo(message);
      startTransition(() => router.refresh());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'update failed');
    } finally {
      setBusyId(null);
    }
  };

  const remove = async (assignment: AssignmentRow) => {
    if (!canDelete || !window.confirm(`Delete assignment #${assignment.id}?`)) return;
    setBusyId(assignment.id);
    setError(null);
    try {
      const { deleteAssignment } = await import('@/app/actions/ai');
      await deleteAssignment(assignment.id);
      setInfo(`Assignment #${assignment.id} deleted.`);
      startTransition(() => router.refresh());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'delete failed');
    } finally {
      setBusyId(null);
    }
  };

  const create = async () => {
    if (!canCreate || !draft.section_key || !draft.provider_id || !draft.model_id) {
      setError('all fields required');
      return;
    }
    setError(null);
    try {
      const { createAssignment } = await import('@/app/actions/ai');
      await createAssignment({ section_key: draft.section_key, task_type: draft.task_type, provider_id: Number(draft.provider_id), model_id: Number(draft.model_id), priority: draft.priority });
      setInfo(`Assignment for ${draft.section_key} created.`);
      setShowCreate(false);
      setDraft((current) => ({ ...current, section_key: '' }));
      startTransition(() => router.refresh());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'create failed');
    }
  };

  const providerModels = models.filter((model) => !draft.provider_id || model.provider_id === Number(draft.provider_id));

  return (
    <Panel padding="none" className="overflow-hidden">
      <div className="space-y-3 border-b border-rule p-4">
        {error && <Alert tone="critical" title="Assignment update failed">{error}</Alert>}
        {info && <Alert tone="positive" title={info} />}
        {canCreate && <div className="flex justify-end"><button type="button" className="h-9 rounded-md bg-accent px-3 text-xs font-medium text-accent-ink hover:bg-accent-strong" onClick={() => setShowCreate((value) => !value)}><T id={showCreate ? 'aiSettings.cancel' : 'aiSettings.newAssignment'} /></button></div>}
        {showCreate && (
          <Panel padding="sm" className="bg-paper">
            <div className="grid gap-2 md:grid-cols-5">
              <input className={INPUT} placeholder="section_key" value={draft.section_key} onChange={(event) => setDraft({ ...draft, section_key: event.target.value })} />
              <select className={INPUT} value={draft.task_type} onChange={(event) => setDraft({ ...draft, task_type: event.target.value as 'embed' | 'chat' | 'vision' })}><option value="chat">chat</option><option value="embed">embed</option><option value="vision">vision</option></select>
              <select className={INPUT} value={draft.provider_id} onChange={(event) => setDraft({ ...draft, provider_id: event.target.value, model_id: '' })}>{providers.map((provider) => <option key={provider.id} value={provider.id}>{provider.name}</option>)}</select>
              <select className={INPUT} value={draft.model_id} onChange={(event) => setDraft({ ...draft, model_id: event.target.value })}><option value="">— model —</option>{providerModels.filter((model) => model.capabilities.includes(draft.task_type)).map((model) => <option key={model.id} value={model.id}>{model.name}</option>)}</select>
              <input type="number" className={INPUT} value={draft.priority} onChange={(event) => setDraft({ ...draft, priority: Number(event.target.value) || 100 })} />
            </div>
            <div className="mt-3 flex justify-end"><button type="button" className="h-9 rounded-md bg-accent px-3 text-xs font-medium text-accent-ink hover:bg-accent-strong" onClick={create}><T id="aiSettings.create" /></button></div>
          </Panel>
        )}
      </div>
      {initialAssignments.length === 0 ? (
        <Empty icon="layers" title={<T id="aiSettings.noAssignments" />} />
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="border-b border-rule bg-paper-3 text-xs uppercase tracking-wider text-mute"><tr><th className="px-4 py-3 text-left"><T id="aiSettings.section" /></th><th className="px-4 py-3 text-left"><T id="aiSettings.task" /></th><th className="px-4 py-3 text-left"><T id="aiSettings.provider" /></th><th className="px-4 py-3 text-left"><T id="aiSettings.model" /></th><th className="px-4 py-3 text-left"><T id="aiSettings.priority" /></th><th className="px-4 py-3 text-right"><T id="aiSettings.actions" /></th></tr></thead>
            <tbody>
              {initialAssignments.map((assignment) => {
                const busy = busyId === assignment.id;
                return (
                  <tr key={assignment.id} className="border-t border-rule hover:bg-paper-3">
                    <td className="px-4 py-3 font-mono text-xs text-ink">{assignment.section_key}</td>
                    <td className="px-4 py-3"><Badge tone="neutral" size="sm">{assignment.task_type}</Badge></td>
                    <td className="px-4 py-3"><select disabled={!canEdit || busy} value={assignment.provider_id ?? ''} onChange={(event) => update(assignment.id, { provider_id: event.target.value ? Number(event.target.value) : null }, `Assignment #${assignment.id} provider updated.`)} className={['w-full', INPUT].join(' ')}><option value="">— none —</option>{providers.map((provider) => <option key={provider.id} value={provider.id}>{provider.name}</option>)}</select></td>
                    <td className="px-4 py-3"><select disabled={!canEdit || busy} value={assignment.model_id ?? ''} onChange={(event) => update(assignment.id, { model_id: event.target.value ? Number(event.target.value) : null }, `Assignment #${assignment.id} model updated.`)} className={['w-full', INPUT].join(' ')}><option value="">— none —</option>{models.filter((model) => !assignment.provider_id || model.provider_id === assignment.provider_id).map((model) => <option key={model.id} value={model.id}>{model.name}</option>)}</select></td>
                    <td className="px-4 py-3 font-mono text-xs tabular-nums text-mute">{assignment.priority}</td>
                    <td className="px-4 py-3 text-right"><div className="inline-flex items-center gap-1"><Status tone={assignment.enabled ? 'positive' : 'neutral'} size="sm">{assignment.enabled ? 'enabled' : 'disabled'}</Status>{canEdit && <button type="button" className="h-8 rounded-md border border-rule bg-paper px-2 text-xs text-ink-2 hover:bg-paper-3 disabled:opacity-40" onClick={() => update(assignment.id, { enabled: !assignment.enabled }, `#${assignment.id} ${!assignment.enabled ? 'enabled' : 'disabled'}.`)} disabled={busy}><T id={assignment.enabled ? 'aiSettings.disable' : 'aiSettings.enable'} /></button>}{canDelete && <button type="button" className="h-8 rounded-md bg-critical px-2 text-xs text-paper hover:bg-critical-strong disabled:opacity-40" onClick={() => remove(assignment)} disabled={busy}><T id="aiSettings.delete" /></button>}</div></td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </Panel>
  );
}
