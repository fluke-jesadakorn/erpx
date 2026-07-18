'use client';

import { useState, useTransition } from 'react';
import { useRouter } from 'next/navigation';
import { Cpu } from 'lucide-react';
import { T } from '@/components/i18n/T';
import { Alert, Badge, Empty, Modal, Panel, Status, useToast } from '@/components/ui';

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
  const toast = useToast();
  const [busyId, setBusyId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [deletingModel, setDeletingModel] = useState<ModelRow | null>(null);
  const [, startTransition] = useTransition();

  const toggle = async (model: ModelRow) => {
    if (!canEdit) return;
    setBusyId(model.id);
    setError(null);
    setInfo(null);
    try {
      const { updateModel } = await import('@/app/actions/ai');
      await updateModel(model.id, { enabled: !model.enabled });
      const msg = `#${model.id} ${!model.enabled ? 'enabled' : 'disabled'}.`;
      setInfo(msg);
      toast.success(msg);
      startTransition(() => router.refresh());
    } catch (cause) {
      const msg = cause instanceof Error ? cause.message : 'update failed';
      setError(msg);
      toast.error(msg);
    } finally {
      setBusyId(null);
    }
  };

  const remove = async (model: ModelRow) => {
    setBusyId(model.id);
    setError(null);
    setInfo(null);
    try {
      const { deleteModel } = await import('@/app/actions/ai');
      await deleteModel(model.id);
      const msg = `Model "${model.name}" deleted.`;
      setInfo(msg);
      toast.success(msg);
      startTransition(() => router.refresh());
    } catch (cause) {
      const msg = cause instanceof Error ? cause.message : 'delete failed';
      setError(msg);
      toast.error(msg);
    } finally {
      setBusyId(null);
      setDeletingModel(null);
    }
  };

  return (
    <Panel padding="none" className="overflow-hidden">
      {(error || info) && <div className="space-y-2 p-4">{error && <Alert tone="critical" title="Model update failed">{error}</Alert>}{info && <Alert tone="positive" title={info} />}</div>}
      {initialModels.length === 0 ? (
        <Empty icon={Cpu} title={<T id="aiSettings.noModels" />} />
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
                        {canDelete && <button type="button" onClick={() => setDeletingModel(model)} disabled={busy} className="h-8 rounded-md bg-critical px-2 text-xs text-paper hover:bg-critical-strong disabled:opacity-40"><T id="aiSettings.delete" /></button>}
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      <Modal
        open={deletingModel !== null}
        onClose={() => (busyId !== null ? null : setDeletingModel(null))}
        title="Delete model"
        subtitle={`Delete model "${deletingModel?.name}"?`}
        tone="rose"
        width="md"
        footer={
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setDeletingModel(null)}
              disabled={busyId !== null}
              className="rounded-lg border border-rule px-3 py-1.5 text-xs text-ink-2 hover:bg-paper-2 disabled:opacity-50"
            >
              <T id="common.cancel" />
            </button>
            <button
              type="button"
              onClick={() => deletingModel && remove(deletingModel)}
              disabled={busyId !== null}
              className="rounded-lg bg-critical px-3 py-1.5 text-xs text-paper hover:bg-critical-strong disabled:opacity-50"
            >
              {busyId !== null ? <T id="aiSettings.deleting" /> : <T id="common.delete" />}
            </button>
          </div>
        }
      >
        <p className="text-sm text-ink-2">
          Model <span className="font-mono text-ink">{deletingModel?.name}</span> will be removed. Assignments referencing it will fail to resolve.
        </p>
      </Modal>
    </Panel>
  );
}
