'use client';

import { useEffect, useRef, useState } from 'react';

type ThinkLevel = 'auto' | 'low' | 'medium' | 'high';

interface SelectableModel {
  id: number;
  name: string;
  providerName: string;
  isFree: boolean;
  reasoningLevels: ThinkLevel[];
}

interface PreferencesResponse {
  models?: SelectableModel[];
  preference?: {
    modelName?: string;
    thinkLevel?: ThinkLevel;
  } | null;
}

interface Props {
  sectionKey: string;
  task?: 'chat' | 'vision' | 'embed';
  modelName?: string;
  thinkLevel?: ThinkLevel;
  onChange?: (model: { id: number; name: string; providerName: string }) => void;
  onThinkChange?: (level: ThinkLevel) => void;
  className?: string;
}

const INPUT = 'h-8 rounded-md border border-rule bg-paper px-2 text-xs text-ink';

async function readPreferencesResponse(response: Response): Promise<PreferencesResponse | null> {
  if (!response.ok) return null;
  const body = await response.text();
  if (!body.trim()) return null;
  try {
    return JSON.parse(body) as PreferencesResponse;
  } catch {
    return null;
  }
}

export function AiModelControl({
  sectionKey,
  task = 'chat',
  modelName,
  thinkLevel = 'auto',
  onChange,
  onThinkChange,
  className,
}: Props) {
  const [models, setModels] = useState<SelectableModel[]>([]);
  const [selected, setSelected] = useState(modelName ?? '');
  const [selectedThink, setSelectedThink] = useState<ThinkLevel>(thinkLevel);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const onChangeRef = useRef(onChange);

  useEffect(() => { onChangeRef.current = onChange; }, [onChange]);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    fetch(`/api/ai/preferences?sectionKey=${encodeURIComponent(sectionKey)}&task=${task}`)
      .then(readPreferencesResponse)
      .then((data) => {
        if (!alive || !data) return;
        const rows = Array.isArray(data.models) ? data.models : [];
        setModels(rows);
        const preference = data.preference;
        if (preference?.modelName) {
          setSelected(preference.modelName);
          const preferred = rows.find((model) => model.name === preference.modelName);
          if (preferred) onChangeRef.current?.({ id: preferred.id, name: preferred.name, providerName: preferred.providerName });
        }
        if (preference?.thinkLevel) setSelectedThink(preference.thinkLevel);
      })
      .catch(() => {})
      .finally(() => alive && setLoading(false));
    return () => { alive = false; };
  }, [sectionKey, task]);

  async function save(model: SelectableModel, level: ThinkLevel) {
    setSaving(true);
    try {
      const response = await fetch('/api/ai/preferences', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ sectionKey, task, modelId: model.id, thinkLevel: level }),
      });
      if (!response.ok) return;
      setSelected(model.name);
      onChange?.({ id: model.id, name: model.name, providerName: model.providerName });
      onThinkChange?.(level);
    } finally {
      setSaving(false);
    }
  }

  async function clear() {
    setSaving(true);
    try {
      await fetch(`/api/ai/preferences?sectionKey=${encodeURIComponent(sectionKey)}`, { method: 'DELETE' });
      setSelected('');
      setSelectedThink('auto');
      onThinkChange?.('auto');
    } finally {
      setSaving(false);
    }
  }

  const current = models.find((model) => model.name === selected);
  const reasoningLevels = current?.reasoningLevels.length ? current.reasoningLevels : ['auto'];

  return (
    <span className={['inline-flex items-center gap-1.5', className].filter(Boolean).join(' ')}>
      <select
        className={INPUT}
        value={selected}
        disabled={loading || saving || models.length === 0}
        aria-label="AI model"
        onChange={(event) => {
          const model = models.find((item) => item.name === event.target.value);
          if (model) void save(model, selectedThink);
        }}
      >
        <option value="">IT default</option>
        {models.map((model) => (
          <option key={model.id} value={model.name}>
            {model.providerName} · {model.name}{model.isFree ? ' · free' : ''}
          </option>
        ))}
      </select>
      <select
        className={INPUT}
        value={selectedThink}
        disabled={loading || saving || !current || task !== 'chat'}
        aria-label="AI thinking level"
        onChange={(event) => {
          const level = event.target.value as ThinkLevel;
          setSelectedThink(level);
          if (current) void save(current, level);
        }}
      >
        {reasoningLevels.map((level) => <option key={level} value={level}>{level} think</option>)}
      </select>
      {selected && <button type="button" className="text-[10px] text-mute hover:text-ink" disabled={saving} onClick={() => void clear()}>auto</button>}
    </span>
  );
}
