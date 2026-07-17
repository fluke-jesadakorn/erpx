'use client';

import { useEffect, useState } from 'react';
import { listModels } from '@/app/actions/ai';

interface ModelRow {
  id: number;
  name: string;
  provider_id: number;
  capabilities: string[];
  context_window: number | null;
  enabled: boolean;
  description: string | null;
}

export interface ModelPickerProps {
  capability: 'embed' | 'chat' | 'vision';
  value: string;
  onChange: (modelName: string, modelId: number) => void;
  storageKey?: string;
  className?: string;
  emptyLabel?: string;
}

export function ModelPicker({
  capability,
  value,
  onChange,
  storageKey,
  className,
  emptyLabel = '— select model —',
}: ModelPickerProps) {
  const [models, setModels] = useState<ModelRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const rows = (await listModels(capability)) as ModelRow[];
        if (!alive) return;
        const filtered = rows.filter((m) => m.enabled && m.capabilities.includes(capability));
        setModels(filtered);
        if (storageKey) {
          try {
            const saved = localStorage.getItem(storageKey);
            const currentName = value || saved;
            const hit = currentName ? filtered.find((m) => m.name === currentName) : undefined;
            if (hit) {
              if (!value) onChange(hit.name, hit.id);
            } else if (filtered.length > 0) {
              const fallback = filtered[0];
              onChange(fallback.name, fallback.id);
            }
          } catch {}
        }
      } catch (e: any) {
        if (alive) setError(e?.message || 'failed');
      } finally {
        if (alive) setLoading(false);
      }
    })();
    return () => { alive = false; };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [capability]);

  const handle = (name: string) => {
    const hit = models.find((m) => m.name === name);
    if (!hit) return;
    if (storageKey) {
      try { localStorage.setItem(storageKey, name); } catch {}
    }
    onChange(name, hit.id);
  };

  const cls = className ?? 'h-8 rounded-md border border-rule bg-paper px-2 text-xs text-ink';

  if (loading) {
    return (
      <select disabled className={cls}>
        <option>Loading…</option>
      </select>
    );
  }
  if (error) {
    return (
      <select disabled className={[cls, 'border-critical text-critical'].join(' ')}>
        <option>{error}</option>
      </select>
    );
  }
  return (
    <select value={value} onChange={(e) => handle(e.target.value)} className={cls}>
      <option value="">{emptyLabel}</option>
      {models.map((m) => (
        <option key={m.id} value={m.name}>
          {m.name}
        </option>
      ))}
    </select>
  );
}