'use client';

import React, { useState, useTransition } from 'react';
import { useRouter } from 'next/navigation';
import { T } from '@/components/i18n/T';
import { useTranslations } from 'next-intl';

interface TileRow {
  id: string;
  display_name: string;
  subtitle: string;
  icon: string;
  accent: string;
  group_name: string;
  href: string;
  view_perm_id: string;
  request_target: string | null;
  sort_order: number;
}

interface DeptRow {
  id: string;
  display_name: string;
  display_name_th: string | null;
  display_name_de: string | null;
}

interface Props {
  initialTiles: TileRow[];
  departments: DeptRow[];
  canEdit: boolean;
}

interface EditState {
  view_perm_id: string;
}

function isDirty(initial: TileRow, edit: EditState): boolean {
  return (initial.view_perm_id ?? '') !== (edit.view_perm_id ?? '');
}

export const TilesClient: React.FC<Props> = ({ initialTiles, departments, canEdit }) => {
  const router = useRouter();
  const t = useTranslations();

  const tileName = (tileId: string, fallback: string) => {
    const key = `tiles.tile.${tileId}.name`;
    const translated = t(key);
    return translated === key ? fallback : translated;
  };

  const [edits, setEdits] = useState<Record<string, EditState>>(() => {
    const m: Record<string, EditState> = {};
    for (const tt of initialTiles) {
      m[tt.id] = { view_perm_id: tt.view_perm_id ?? '' };
    }
    return m;
  });
  const [savingId, setSavingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [, startTransition] = useTransition();

  const setViewPerm = (id: string, view_perm_id: string) => {
    setEdits((prev) => ({
      ...prev,
      [id]: { ...(prev[id] || { view_perm_id: '' }), view_perm_id },
    }));
    setError(null);
  };

  const resetRow = (id: string) => {
    const tt = initialTiles.find((x) => x.id === id);
    if (!tt) return;
    setEdits((prev) => ({
      ...prev,
      [id]: { view_perm_id: tt.view_perm_id ?? '' },
    }));
    setError(null);
  };

  const save = async (id: string) => {
    if (!canEdit) return;
    const e = edits[id];
    if (!e) return;
    setSavingId(id);
    setError(null);
    try {
      const res = await fetch(`/api/perm/tiles/${id}/gate`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ view_perm_id: e.view_perm_id }),
      });
      if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        throw new Error(data.error || `HTTP ${res.status}`);
      }
      startTransition(() => router.refresh());
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Save failed');
    } finally {
      setSavingId(null);
    }
  };

  return (
    <div className="space-y-3">
      {error ? <div className="text-rose-300 text-xs">{error}</div> : null}
      <table className="w-full text-sm border-separate border-spacing-y-1">
        <thead>
          <tr className="text-xs uppercase tracking-wider text-slate-400">
            <th className="text-left px-2 py-1">
              <T id="tiles.colTile" />
            </th>
            <th className="text-left px-2 py-1">
              <T id="tiles.colGroup" />
            </th>
            <th className="text-left px-2 py-1">view_perm_id</th>
            <th className="text-right px-2 py-1">
              <T id="tiles.colActions" />
            </th>
          </tr>
        </thead>
        <tbody>
          {initialTiles.map((tt) => {
            const dirty = isDirty(tt, edits[tt.id] || { view_perm_id: '' });
            const saving = savingId === tt.id;
            return (
              <tr key={tt.id} className="bg-slate-900/50 border border-slate-700">
                <td className="px-2 py-1.5 text-slate-100">{tileName(tt.id, tt.display_name)}</td>
                <td className="px-2 py-1.5 text-slate-400 text-xs">{tt.group_name}</td>
                <td className="px-2 py-1.5">
                  <input
                    type="text"
                    className="w-full px-2 py-1 rounded bg-slate-950 border border-slate-700 text-xs font-mono text-slate-100"
                    value={edits[tt.id]?.view_perm_id ?? ''}
                    onChange={(e) => setViewPerm(tt.id, e.target.value)}
                    disabled={!canEdit}
                  />
                </td>
                <td className="px-2 py-1.5 text-right">
                  {canEdit ? (
                    <div className="inline-flex gap-2">
                      <button
                        type="button"
                        className="px-2 py-1 rounded bg-slate-700 hover:bg-slate-600 text-xs disabled:opacity-40"
                        onClick={() => resetRow(tt.id)}
                        disabled={!dirty || saving}
                      >
                        <T id="common.cancel" />
                      </button>
                      <button
                        type="button"
                        className="px-2 py-1 rounded bg-cyan-600 hover:bg-cyan-500 text-xs disabled:opacity-40"
                        onClick={() => save(tt.id)}
                        disabled={!dirty || saving}
                      >
                        {saving ? <T id="common.saving" /> : <T id="common.save" />}
                      </button>
                    </div>
                  ) : null}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {departments.length > 0 ? (
        <details className="text-xs text-slate-400">
          <summary className="cursor-pointer">
            <T id="tiles.knownDepartments" values={{ n: departments.length }} />
          </summary>
          <ul className="mt-1 space-y-0.5 pl-4">
            {departments.map((d) => (
              <li key={d.id}>• {d.id} — {d.display_name}</li>
            ))}
          </ul>
        </details>
      ) : null}
    </div>
  );
};