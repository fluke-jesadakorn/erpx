'use client';

import React, { useCallback, useMemo, useState } from 'react';
import { useRouter } from 'next/navigation';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import { useT } from '@/components/i18n/useT';
import { T, interpolate } from '@/components/i18n/T';
import permsDict from '@folio-lib/i18n/permissions';
import { PolicyLintButton } from './PolicyLintButton';

interface PolicyRow {
  id: string;
  name: string;
}

interface Stage {
  perm: string;
  stage: string;
}

interface Persona {
  id: string;
  display_name: string;
  display_name_th?: string | null;
  display_name_de?: string | null;
  sort_order: number;
  level: number;
  user_count: number;
  grants: string[];
}

interface Props {
  stages: Stage[];
  personas: Persona[];
  canEdit: boolean;
  actorName: string;
  policies: PolicyRow[];
}

function personaLabel(p: Persona, locale: 'th' | 'de'): string {
  if (locale === 'de') return p.display_name_de ?? p.display_name;
  return p.display_name_th ?? p.display_name;
}

const STAGE_LABELS: Record<string, { icon: string }> = {
  dept_verification:          { icon: '👥' },
  dept_authorization:         { icon: '🛡️' },
  accounting_verification:    { icon: '🧾' },
  accounting_supervision:     { icon: '📊' },
  accounting_authorization:   { icon: '⚙️' },
  disbursement_authorization: { icon: '💳' },
  cfo_authorization:          { icon: '👑' },
  ceo_authorization:          { icon: '🦅' },
};

export function PolicyMatrixPage({ stages, personas, canEdit, actorName, policies }: Props) {
  const router = useRouter();
  const locale = useSecondaryLocale();
  const t = useT(permsDict);
  const [pending, setPending] = useState<Record<string, Set<string>>>(() => {
    const out: Record<string, Set<string>> = {};
    for (const p of personas) out[p.id] = new Set(p.grants);
    return out;
  });
  const [savingRole, setSavingRole] = useState<string | null>(null);
  const [hoverRole, setHoverRole] = useState<string | null>(null);

  const dirty = useMemo(() => {
    const roles: string[] = [];
    for (const p of personas) {
      const before = new Set(p.grants);
      const after = pending[p.id] ?? new Set();
      if (before.size !== after.size) {
        roles.push(p.id);
        continue;
      }
      for (const v of before) if (!after.has(v)) { roles.push(p.id); break; }
      if (roles.includes(p.id)) continue;
      for (const v of after) if (!before.has(v)) { roles.push(p.id); break; }
    }
    return roles;
  }, [pending, personas]);

  const toggle = useCallback((roleId: string, permId: string) => {
    if (!canEdit) return;
    setPending((prev) => {
      const cur = new Set(prev[roleId] ?? []);
      if (cur.has(permId)) cur.delete(permId); else cur.add(permId);
      return { ...prev, [roleId]: cur };
    });
  }, [canEdit]);

  const save = useCallback(async (roleId: string) => {
    const role = personas.find((p) => p.id === roleId);
    if (!role) return;
    setSavingRole(roleId);
    try {
      const allow = Array.from(pending[roleId] ?? []);
      const r = await fetch(`/api/perm/roles/${encodeURIComponent(roleId)}/permissions`, {
        method: 'PUT',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ allow }),
      });
      if (!r.ok) {
        const j = await r.json().catch(() => ({}));
        alert(`Save failed: ${j.error || r.statusText}`);
      } else {
        router.refresh();
      }
    } finally {
      setSavingRole(null);
    }
  }, [pending, personas, router]);

  const revert = useCallback((roleId: string) => {
    const role = personas.find((p) => p.id === roleId);
    if (!role) return;
    setPending((prev) => ({ ...prev, [roleId]: new Set(role.grants) }));
  }, [personas]);

  const totalGrants = personas.reduce((n, p) => n + (pending[p.id]?.size ?? 0), 0);

  return (
    <div className="space-y-5 animate-fade-in">
      <header className="flex items-end justify-between gap-4 flex-wrap">
        <div className="min-w-0">
          <h2 className="text-base font-black uppercase tracking-widest text-slate-100 flex items-center gap-2">
            <span aria-hidden>🛂</span>
            <T value={t('matrix.title')} /> · <T value={interpolate(t('matrix.grantsN'), { n: totalGrants })} />
          </h2>
          <p className="mt-1 text-xs text-slate-400 leading-relaxed">
            <T value={t('matrix.subtitle')} />
          </p>
          <div className="mt-2 flex items-center gap-2 flex-wrap">
            <span className="text-xs font-mono uppercase tracking-widest text-slate-500">
              <T value={t('matrix.chainOrder')} />
            </span>
            {stages.map((s, i) => (
              <React.Fragment key={s.perm}>
                <span className="text-xs font-mono text-indigo-300">
                  {STAGE_LABELS[s.stage]?.icon ?? '·'} <T value={t(`stage.${s.stage}`)} />
                </span>
                {i < stages.length - 1 && <span className="text-slate-600">→</span>}
              </React.Fragment>
            ))}
          </div>
        </div>
        <div className="text-xs font-mono text-slate-500">
          {canEdit ? <T value={t('matrix.editGranted')} /> : <T value={t('matrix.viewOnly')} />} · {actorName || '—'}
        </div>
      </header>

      <div className="rounded-2xl border border-slate-800 bg-slate-950/60 overflow-hidden">
        <div className="overflow-x-auto">
          <table className="w-full text-xs">
            <thead className="bg-slate-900/80 border-b border-slate-800">
              <tr>
                <th className="text-left px-3 py-2.5 sticky left-0 bg-slate-900/95 backdrop-blur z-10 min-w-[200px]">
                  <span className="text-xs font-black uppercase tracking-widest text-slate-300">
                    <T value={t('matrix.colPersona')} />
                  </span>
                </th>
                {stages.map((s) => (
                  <th key={s.perm} className="px-2 py-2.5 min-w-[110px] text-center">
                    <div className="text-xs font-mono text-slate-200 font-bold flex flex-col items-center gap-0.5">
                      <span className="text-base leading-none">{STAGE_LABELS[s.stage]?.icon ?? '·'}</span>
                      <span className="leading-tight">
                        <T value={t(`stage.${s.stage}`)} />
                      </span>
                    </div>
                  </th>
                ))}
                <th className="px-3 py-2.5 min-w-[120px] text-right">
                  <span className="text-xs font-mono uppercase tracking-widest text-slate-400">
                    <T value={t('matrix.colUsers')} />
                  </span>
                </th>
                {canEdit && (
                  <th className="px-3 py-2.5 min-w-[160px] text-right">
                    <span className="text-xs font-mono uppercase tracking-widest text-slate-400">
                      <T value={t('matrix.colActions')} />
                    </span>
                  </th>
                )}
              </tr>
            </thead>
            <tbody>
              {personas.map((p) => {
                const cells = pending[p.id] ?? new Set<string>();
                const isDirty = dirty.includes(p.id);
                const saving = savingRole === p.id;
                return (
                  <tr
                    key={p.id}
                    onMouseEnter={() => setHoverRole(p.id)}
                    onMouseLeave={() => setHoverRole((r) => (r === p.id ? null : r))}
                    className={[
                      'border-t border-slate-800/60 transition-colors',
                      hoverRole === p.id ? 'bg-slate-900/40' : '',
                      isDirty ? 'bg-amber-950/10' : '',
                    ].join(' ')}
                  >
                    <td className="px-3 py-2 sticky left-0 bg-slate-950/95 backdrop-blur z-10">
                      <div className="flex flex-col">
                        <span className="font-bold text-slate-100">{personaLabel(p, locale)}</span>
                        <span className="text-xs font-mono text-slate-500">{p.id}</span>
                      </div>
                    </td>
                    {stages.map((s) => {
                      const on = cells.has(s.perm);
                      return (
                        <td key={s.perm} className="px-2 py-2 text-center">
                          <button
                            type="button"
                            disabled={!canEdit || saving}
                            onClick={() => toggle(p.id, s.perm)}
                            aria-pressed={on}
                            aria-label={`${personaLabel(p, locale)} ${on ? 'has' : 'does not have'} ${s.stage.replace(/_/g, ' ')}`}
                            className={[
                              'w-full h-9 rounded-lg border text-sm font-bold font-mono transition-all',
                              on
                                ? 'bg-indigo-500/25 border-indigo-400/60 text-indigo-100 shadow-[0_0_12px_rgba(99,102,241,0.25)]'
                                : 'bg-slate-900/60 border-slate-800 text-slate-500',
                              canEdit && !saving ? 'cursor-pointer hover:border-indigo-500/60' : 'cursor-default',
                              saving ? 'opacity-50' : '',
                            ].join(' ')}
                          >
                            {on ? '✓' : '—'}
                          </button>
                        </td>
                      );
                    })}
                    <td className="px-3 py-2 text-right">
                      <span className="text-sm font-mono text-slate-300">
                        {p.user_count}
                      </span>
                    </td>
                    {canEdit && (
                      <td className="px-3 py-2 text-right">
                        <div className="inline-flex items-center gap-1.5">
                          <button
                            type="button"
                            disabled={!isDirty || saving}
                            onClick={() => revert(p.id)}
                            className="text-xs font-mono px-2 py-1 rounded border border-slate-700 text-slate-300 hover:bg-slate-800 disabled:opacity-30"
                          >
                            <T value={t('matrix.reset')} />
                          </button>
                          <button
                            type="button"
                            disabled={!isDirty || saving}
                            onClick={() => save(p.id)}
                            className={[
                              'text-xs font-mono px-2 py-1 rounded border font-bold',
                              isDirty
                                ? 'bg-indigo-500/30 border-indigo-500/60 text-indigo-100 hover:bg-indigo-500/40'
                                : 'border-slate-800 text-slate-600',
                              saving ? 'opacity-60' : '',
                            ].join(' ')}
                          >
                            {saving ? <T value={t('matrix.saving')} /> : <T value={t('matrix.save')} />}
                          </button>
                        </div>
                      </td>
                    )}
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>

      <div className="rounded-2xl border border-slate-800 bg-slate-950/40 p-4 text-sm text-slate-400 leading-relaxed">
        <span className="text-xs uppercase tracking-widest font-bold text-slate-300 mr-2">
          <T value={t('matrix.howItWorks')} />
        </span>
        <T value={t('matrix.howItWorksBody')} />
      </div>

      <PolicyLintButton policies={policies} />
    </div>
  );
}
