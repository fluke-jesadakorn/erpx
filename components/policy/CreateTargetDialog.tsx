'use client';

import React, { useState } from 'react';
import { T } from '@/components/i18n/T';

interface Props {
  onClose: () => void;
  onCreated: () => void;
}

type Kind = 'department' | 'role';

export function CreateTargetDialog({ onClose, onCreated }: Props) {
  const [kind, setKind] = useState<Kind>('department');
  const [id, setId] = useState('');
  const [label, setLabel] = useState('');
  const [level, setLevel] = useState(5);
  const [saving, setSaving] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  const save = async () => {
    setSaving(true);
    setMsg(null);
    try {
      const r = await fetch('/api/policy/targets', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ kind, id, label, level }),
      });
      const j = await r.json().catch(() => ({}));
      if (!r.ok) throw new Error(j.error || `HTTP ${r.status}`);
      onCreated();
      onClose();
    } catch (e: unknown) {
      setMsg(e instanceof Error ? e.message : 'Save failed');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 bg-slate-950/60 flex items-center justify-center p-4">
      <div className="w-full max-w-md bg-white border border-slate-300 shadow-xl">
        <h3 className="text-base font-black uppercase tracking-widest text-slate-100">
          <T id="policy.matrix2.createTarget" hideSecondary />
        </h3>

        <div className="flex gap-2">
          {(['department', 'role'] as Kind[]).map((k) => (
            <button
              key={k}
              type="button"
              onClick={() => setKind(k)}
              className={[
                'flex-1 px-3 py-2 rounded text-xs font-mono uppercase border',
                kind === k
                  ? 'bg-indigo-500/25 border-indigo-400/60 text-indigo-100'
                  : 'border-slate-700 text-slate-400 hover:bg-slate-900/60',
              ].join(' ')}
            >
              {k}
            </button>
          ))}
        </div>

        <label className="block">
          <span className="text-[10px] font-mono uppercase tracking-widest text-slate-400">
            id
          </span>
          <input
            value={id}
            onChange={(e) => setId(e.target.value)}
            placeholder={kind === 'department' ? 'sales' : 'regional_supervisor'}
            className="w-full mt-1 px-3 py-2 rounded-md text-xs font-mono bg-slate-950 border border-slate-700 text-slate-100"
          />
        </label>

        <label className="block">
          <span className="text-[10px] font-mono uppercase tracking-widest text-slate-400">
            label
          </span>
          <input
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder={kind === 'department' ? 'Sales' : 'Regional Supervisor'}
            className="w-full mt-1 px-3 py-2 rounded-md text-xs font-mono bg-slate-950 border border-slate-700 text-slate-100"
          />
        </label>

        {kind === 'role' ? (
          <label className="block">
            <span className="text-[10px] font-mono uppercase tracking-widest text-slate-400">
              level (1-10)
            </span>
            <input
              type="number"
              min={1}
              max={10}
              value={level}
              onChange={(e) => setLevel(Number(e.target.value))}
              className="w-full mt-1 px-3 py-2 rounded-md text-xs font-mono bg-slate-950 border border-slate-700 text-slate-100"
            />
          </label>
        ) : null}

        {msg ? <div className="text-xs text-rose-300">{msg}</div> : null}

        <div className="flex justify-end gap-2 pt-2">
          <button
            type="button"
            onClick={onClose}
            className="px-3 py-1.5 rounded text-xs font-mono border border-slate-700 text-slate-300 hover:bg-slate-800"
          >
            <T id="policy.matrix2.cancel" hideSecondary />
          </button>
          <button
            type="button"
            disabled={!id || saving}
            onClick={save}
            className="px-3 py-1.5 rounded text-xs font-mono border border-indigo-500/60 bg-indigo-500/30 text-indigo-100 hover:bg-indigo-500/40 disabled:opacity-40"
          >
            {saving ? 'Saving…' : <T id="policy.matrix2.create" hideSecondary />}
          </button>
        </div>
      </div>
    </div>
  );
}
