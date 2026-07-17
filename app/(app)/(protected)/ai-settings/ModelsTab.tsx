'use client';

import { useState, useTransition } from 'react';
import { useRouter } from 'next/navigation';
import { T } from '@/components/i18n/T';
import { Alert, Badge, Empty, Panel, Status } from '@/components/ui';

interface ModelRow {
  id: number;
  name: string;
  provider_id: number;
  capabilities: string[];
  context_window: number | null;
  enabled: boolean;
  description: string | null;
}

interface Props {
  initialModels: ModelRow[];
  providers: { id: number; name: string }[];
  canEdit: boolean;
  canDelete: boolean;
}

export function ModelsTab({ initialModels, providers, canEdit, canDelete }: Props) {
  const router = useRouter();
  const [busyId, setBusyId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [, startTransition] = useTransition();

  const toggle = async (model: ModelRow) => {
    if (!canEdit) return;
    setBusyId(model.id);
    setError(null);
    setInfo(null);
    try {
      const { updateModel } = await import('@/app/actions/ai');
      await updateModel(model.id, { enabled: !model.enabled });
      setInfo(`#${model.id} ${!model.enabled ? 'enabled' : 'disabled'}.`);
      startTransition(() => router.refresh());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'update failed');
    } finally {
      setBusyId(null);
    }
  };

  const remove = async (model: ModelRow) => {
    if (!canDelete || !window.confirm(`Delete model "${model.name}"?`)) return;
    setBusyId(model.id);
    setError(null);
    setInfo(null);
    try {
      const { deleteModel } = await import('@/app/actions/ai');
      await deleteModel(model.id);
      setInfo(`Model "${model.name}" deleted.`);
      startTransition(() => router.refresh());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'delete failed');
    } finally {
      setBusyId(null);
    }
  };

  return (
    <Panel padding="none" className="overflow-hidden">
      {(error || info) && <div className="space-y-2 p-4">{error && <Alert tone="critical" title="Model update failed">{error}</Alert>}{info && <Alert tone="positive" title={info} />}</div>}
      {initialModels.length === 0 ? (
        <Empty icon="cpu" title={<T id="aiSettings.noModels" />} />
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="border-b border-rule bg-paper-3 text-xs uppercase tracking-wider text-mute">
              <tr><th className="px-4 py-3 text-left">#</th><th className="px-4 py-3 text-left"><T id="aiSettings.colName" /></th><th className="px-4 py-3 text-left"><T id="aiSettings.provider" /></th><th className="px-4 py-3 text-left"><T id="aiSettings.capabilities" /></th><th className="px-4 py-3 text-left"><T id="aiSettings.context" /></th><th className="px-4 py-3 text-right"><T id="aiSettings.actions" /></th></tr>
            </thead>
            <tbody>
              {initialModels.map((model) => {
                const busy = busyId === model.id;
                return (
                  <tr key={model.id} className="border-t border-rule hover:bg-paper-3">
                    <td className="px-4 py-3 font-mono text-xs tabular-nums text-mute">{model.id}</td>
                    <td className="px-4 py-3 font-mono text-xs text-ink">{model.name}</td>
                    <td className="px-4 py-3 text-xs text-ink-2">{providers.find((provider) => provider.id === model.provider_id)?.name ?? '?'}</td>
                    <td className="px-4 py-3"><div className="flex flex-wrap gap-1">{model.capabilities.map((capability) => <Badge key={capability} tone="neutral" size="sm">{capability}</Badge>)}</div></td>
                    <td className="px-4 py-3 font-mono text-xs tabular-nums text-mute">{model.context_window ?? '—'}</td>
                    <td className="px-4 py-3 text-right">
                      <div className="inline-flex items-center gap-1">
                        <Status tone={model.enabled ? 'positive' : 'neutral'} size="sm">{model.enabled ? 'enabled' : 'disabled'}</Status>
                        {canEdit && <button type="button" onClick={() => toggle(model)} disabled={busy} className="h-8 rounded-md border border-rule bg-paper px-2 text-xs text-ink-2 hover:bg-paper-3 disabled:opacity-40"><T id={model.enabled ? 'aiSettings.disable' : 'aiSettings.enable'} /></button>}
                        {canDelete && <button type="button" onClick={() => remove(model)} disabled={busy} className="h-8 rounded-md bg-critical px-2 text-xs text-paper hover:bg-critical-strong disabled:opacity-40"><T id="aiSettings.delete" /></button>}
                      </div>
                    </td>
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
