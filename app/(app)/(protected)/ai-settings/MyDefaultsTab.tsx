'use client';

import { useState, useTransition } from 'react';
import { useRouter } from 'next/navigation';
import { T } from '@/components/i18n/T';
import { SECTION_CATALOG } from '@folio-lib/ai/sections';
import { Alert, Badge, Panel } from '@/components/ui';

interface DefaultRow {
  section_key: string;
  model_id: number;
  model_name: string;
  provider_name: string | null;
}

interface Props {
  initialDefaults: DefaultRow[];
  models: { id: number; name: string; capabilities: string[]; provider_id: number }[];
  providers: { id: number; name: string }[];
  canWrite: boolean;
}

export function MyDefaultsTab({ initialDefaults, models, providers, canWrite }: Props) {
  const router = useRouter();
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [, startTransition] = useTransition();
  const defaults = new Map(initialDefaults.map((item) => [item.section_key, item]));

  const save = async (sectionKey: string, modelId: number) => {
    if (!canWrite) return;
    setBusyKey(sectionKey);
    setError(null);
    try {
      const { setMyDefault } = await import('@/app/actions/ai');
      await setMyDefault(sectionKey, modelId);
      setInfo(`Default for ${sectionKey} updated.`);
      startTransition(() => router.refresh());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'save failed');
    } finally {
      setBusyKey(null);
    }
  };

  const clear = async (sectionKey: string) => {
    if (!canWrite) return;
    setBusyKey(sectionKey);
    setError(null);
    try {
      const { deleteMyDefault } = await import('@/app/actions/ai');
      await deleteMyDefault(sectionKey);
      setInfo(`Default for ${sectionKey} cleared.`);
      startTransition(() => router.refresh());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'delete failed');
    } finally {
      setBusyKey(null);
    }
  };

  return (
    <Panel padding="none" className="overflow-hidden">
      <div className="space-y-2 border-b border-rule p-4">
        {error && <Alert tone="critical" title="Default update failed">{error}</Alert>}
        {info && <Alert tone="positive" title={info} />}
        <p className="text-xs text-mute"><T id="aiSettings.myDefaultsHint" /></p>
      </div>
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="border-b border-rule bg-paper-3 text-xs uppercase tracking-wider text-mute">
            <tr><th className="px-4 py-3 text-left"><T id="aiSettings.section" /></th><th className="px-4 py-3 text-left"><T id="aiSettings.task" /></th><th className="px-4 py-3 text-left"><T id="aiSettings.yourDefault" /></th><th className="px-4 py-3 text-right"><T id="aiSettings.actions" /></th></tr>
          </thead>
          <tbody>
            {SECTION_CATALOG.map((section) => {
              const current = defaults.get(section.key);
              const busy = busyKey === section.key;
              return (
                <tr key={section.key} className="border-t border-rule hover:bg-paper-3">
                  <td className="px-4 py-3"><div className="font-mono text-xs text-ink">{section.key}</div><div className="mt-1 text-xs text-mute">{section.label}</div></td>
                  <td className="px-4 py-3"><Badge tone="neutral" size="sm">{section.task}</Badge></td>
                  <td className="px-4 py-3">
                    <select disabled={!canWrite || busy} value={current?.model_id ?? ''} onChange={(event) => event.target.value && save(section.key, Number(event.target.value))} className="h-9 w-full rounded-md border border-rule bg-paper px-2 text-xs text-ink focus:border-accent">
                      <option value="">— <T id="aiSettings.inheritDefault" /> —</option>
                      {models.filter((model) => model.capabilities.includes(section.task)).map((model) => <option key={model.id} value={model.id}>{model.name} ({providers.find((provider) => provider.id === model.provider_id)?.name ?? '?'})</option>)}
                    </select>
                  </td>
                  <td className="px-4 py-3 text-right">
                    {current ? <button type="button" disabled={!canWrite || busy} onClick={() => clear(section.key)} className="h-8 rounded-md border border-rule bg-paper px-2 text-xs text-ink-2 hover:bg-paper-3 disabled:opacity-40"><T id="aiSettings.clear" /></button> : <span className="text-xs text-mute"><T id="aiSettings.inheritDefault" /></span>}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </Panel>
  );
}
