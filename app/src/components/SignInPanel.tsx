'use client';

import React, { useEffect, useState } from 'react';
import { useRouter } from 'next/navigation';
import { useToast } from '@/components/ui/Toast';
import { ROLE_RANK, ROLE_LEVEL, type DisplayRoleName, type StaffLevel } from '@folio-lib/org/display';

interface SeedUser {
  id: number;
  employee_code: string;
  fullname: string;
  department: string | null;
  dept_group_id?: string | null;
  dept_group_name?: string | null;
  role_name: string;
  role_id: string;
  level?: number;
}

const ROLE_ACCENT: Record<string, string> = {
  staff:               'from-emerald-500/25 via-emerald-700/15 to-emerald-900/40 border-emerald-500/30',
  officer:             'from-emerald-500/25 via-emerald-700/15 to-emerald-900/40 border-emerald-500/30',
  sales_rep:           'from-emerald-500/25 via-emerald-700/15 to-emerald-900/40 border-emerald-500/30',
  accountant:          'from-cyan-500/25 via-cyan-700/15 to-cyan-900/40 border-cyan-500/30',
  account_officer:     'from-cyan-500/25 via-cyan-700/15 to-cyan-900/40 border-cyan-500/30',
  account_supervisor:  'from-cyan-500/25 via-cyan-700/15 to-cyan-900/40 border-cyan-500/30',
  accounting_manager:  'from-cyan-500/25 via-cyan-700/15 to-cyan-900/40 border-cyan-500/30',
  supervisor:          'from-amber-400/25 via-amber-700/15 to-amber-900/40 border-amber-500/40',
  sales_supervisor:    'from-cyan-500/25 via-cyan-700/15 to-cyan-900/40 border-cyan-500/30',
  head_of_department:  'from-amber-400/25 via-amber-700/15 to-amber-900/40 border-amber-500/40',
  manager:             'from-amber-400/25 via-amber-700/15 to-amber-900/40 border-amber-500/40',
  admin:               'from-purple-500/25 via-purple-700/15 to-purple-900/40 border-purple-500/30',
  cfo:                 'from-purple-500/25 via-purple-700/15 to-purple-900/40 border-purple-500/30',
  finance:             'from-purple-500/25 via-purple-700/15 to-purple-900/40 border-purple-500/30',
  ceo:                 'from-rose-500/25 via-rose-700/15 to-rose-900/40 border-rose-500/40',
  it:                  'from-slate-500/25 via-slate-700/15 to-slate-900/40 border-slate-500/40',
  hr:                  'from-indigo-500/25 via-indigo-700/15 to-indigo-900/40 border-indigo-500/30',
  hr_manager:          'from-indigo-500/25 via-indigo-700/15 to-indigo-900/40 border-indigo-500/30',
};

const ROLE_LABEL: Record<string, string> = {
  staff: 'Staff',
  officer: 'Officer',
  sales_rep: 'Sales Representative',
  accountant: 'Accountant',
  account_officer: 'Account Officer',
  account_supervisor: 'Account Supervisor',
  accounting_manager: 'Accounting Manager',
  supervisor: 'Supervisor',
  sales_supervisor: 'Sales Supervisor',
  head_of_department: 'Head of Department',
  manager: 'Manager',
  admin: 'System Admin',
  cfo: 'Chief Financial Officer',
  finance: 'Financial Officer',
  ceo: 'Chief Executive Officer',
  it: 'IT Manager',
  hr: 'HR Officer',
  hr_manager: 'HR Manager',
};

const LEVEL_ORDER: StaffLevel[] = [1, 2, 3, 4, 5];

const LEVEL_META: Record<StaffLevel, { th: string; icon: string; accent: string }> = {
  1: { th: 'P1 · Executive',         icon: '👑', accent: 'bg-rose-500/10 text-rose-200 border-rose-500/30' },
  2: { th: 'P2 · Senior Management', icon: '🛡️', accent: 'bg-purple-500/10 text-purple-200 border-purple-500/30' },
  3: { th: 'P3 · Middle Management', icon: '🧭', accent: 'bg-amber-500/10 text-amber-200 border-amber-500/30' },
  4: { th: 'P4 · Senior Staff',      icon: '👥', accent: 'bg-cyan-500/10 text-cyan-200 border-cyan-500/30' },
  5: { th: 'P5 · Staff',             icon: '📋', accent: 'bg-emerald-500/10 text-emerald-200 border-emerald-500/30' },
};

function staffLevelOf(u: SeedUser): StaffLevel {
  const lv = u.level;
  if (lv === 1 || lv === 2 || lv === 3 || lv === 4 || lv === 5) return lv;
  const role = u.role_id || '';
  return ROLE_LEVEL[role as DisplayRoleName] ?? 5;
}

export const SignInPanel: React.FC<{ next?: string }> = ({ next: _next = '/' } = {}) => {
  const router = useRouter();
  const toast = useToast();
  const [users, setUsers] = useState<SeedUser[]>([]);
  const [pinRequired, setPinRequired] = useState(false);
  const [pin, setPin] = useState('');
  const [busyId, setBusyId] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    (async () => {
      try {
        const res = await fetch('/api/actor/users');
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const data = await res.json();
        if (mounted) {
          setUsers(data.users || []);
          setPinRequired(!!data.pinRequired);
        }
      } catch (e: any) {
        if (mounted) setError(e?.message || 'Failed to load users');
      } finally {
        if (mounted) setLoading(false);
      }
    })();
    return () => { mounted = false; };
  }, []);

  const levelBuckets = React.useMemo(() => {
    const buckets: Record<StaffLevel, SeedUser[]> = { 1: [], 2: [], 3: [], 4: [], 5: [] };
    for (const u of users) {
      const lv = staffLevelOf(u);
      buckets[lv].push(u);
    }
    for (const lv of LEVEL_ORDER) {
      buckets[lv].sort((a, b) => {
        const rankA = ROLE_RANK[a.role_id as DisplayRoleName] ?? 99;
        const rankB = ROLE_RANK[b.role_id as DisplayRoleName] ?? 99;
        if (rankA !== rankB) return rankA - rankB;
        return (a.fullname || '').localeCompare(b.fullname || '');
      });
    }
    return buckets;
  }, [users]);

  async function signIn(user: SeedUser) {
    if (pinRequired && !pin.trim()) {
      toast.error('Enter the dev PIN to sign in.');
      return;
    }
    setBusyId(user.id);
    try {
      const res = await fetch('/api/actor', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ id: user.id, pin: pinRequired ? pin : undefined }),
      });
      const data = await res.json().catch(() => ({}));
      if (!res.ok) {
        toast.error(data?.error || `Sign-in failed (${res.status})`);
        return;
      }
      toast.success(`Signed in as ${user.fullname}`, data.role);
      router.refresh();
    } catch (e: any) {
      toast.error(e?.message || 'Sign-in failed');
    } finally {
      setBusyId(null);
    }
  }

  return (
    <div className="min-h-screen relative text-slate-100 selection:bg-indigo-500 selection:text-white pb-16">
      <div className="absolute top-10 left-10 w-96 h-96 bg-indigo-500/10 rounded-full blur-3xl pointer-events-none z-0" />
      <div className="absolute top-60 right-10 w-96 h-96 bg-purple-500/10 rounded-full blur-3xl pointer-events-none z-0" />
      <main className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8 py-10 relative z-10">
        <div className="mb-8">
          <h1 className="text-2xl font-black tracking-tight text-white">Folio</h1>
          <p className="text-sm text-slate-400 mt-1">
            Dev persona switcher — choose a seeded user to sign in.
          </p>
        </div>

        {loading && (
          <div className="glass-panel rounded-2xl p-6 text-sm text-slate-400">Loading users…</div>
        )}
        {error && (
          <div className="glass-panel rounded-2xl p-6 border-rose-500/30">
            <div className="text-sm text-rose-300 font-mono">Failed to load users: {error}</div>
            <p className="text-xs text-slate-500 mt-2">
              Make sure <code className="font-mono">db/seed.sql</code> +{' '}
              <code className="font-mono">db/v2_seed.sql</code> are applied and the DB is reachable.
            </p>
          </div>
        )}

        {!loading && !error && (
          <>
            {pinRequired && (
              <div className="glass-panel rounded-2xl p-4 border-amber-500/30 mb-6">
                <label className="block">
                  <span className="text-xs font-mono uppercase tracking-wider text-slate-400">
                    DEV_ACTOR_PIN
                  </span>
                  <input
                    type="password"
                    value={pin}
                    onChange={(e) => setPin(e.target.value)}
                    placeholder="Dev PIN (from .env.local)"
                    className="mt-1 w-full rounded-xl bg-slate-950/70 border border-slate-800 px-3 py-2 text-[13px] text-slate-100 placeholder:text-slate-600 focus:outline-none focus:border-amber-500/60"
                  />
                </label>
              </div>
            )}

            <div className="space-y-10">
              {LEVEL_ORDER.map((lv) => {
                const groupUsers = levelBuckets[lv];
                if (groupUsers.length === 0) return null;
                const meta = LEVEL_META[lv];
                return (
                  <div key={lv} className="space-y-4">
                    <div className={`inline-flex items-center gap-2 px-3 py-1.5 rounded-xl border text-xs font-mono font-bold uppercase tracking-wider ${meta.accent}`}>
                      <span>{meta.icon}</span>
                      <span>{meta.th}</span>
                    </div>
                    <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-3">
                      {groupUsers.map((u) => {
                        const tone = ROLE_ACCENT[u.role_id] || ROLE_ACCENT.staff;
                        return (
                          <button
                            key={u.id}
                            type="button"
                            disabled={busyId !== null}
                            onClick={() => signIn(u)}
                            className={`group relative rounded-2xl border bg-gradient-to-br p-4 text-left transition-all hover:scale-[1.015] hover:shadow-lg hover:shadow-black/40 disabled:opacity-50 disabled:cursor-wait ${tone}`}
                          >
                            <div className="flex items-start justify-between gap-2">
                              <div className="min-w-0">
                                <div className="text-base font-black text-white truncate">{u.fullname}</div>
                                <div className="text-xs font-mono text-slate-400 mt-0.5">
                                  {u.employee_code} · {u.dept_group_name ?? u.department ?? '—'}
                                </div>
                              </div>
                              <span className="text-xs font-mono uppercase tracking-wider px-1.5 py-0.5 rounded-full bg-slate-950/60 border border-slate-700 text-slate-200 shrink-0">
                                {ROLE_LABEL[u.role_id] || u.role_name}
                              </span>
                            </div>
                            <div className="mt-3 flex items-center justify-between">
                              <span className="text-xs text-slate-500 font-mono">
                                {busyId === u.id ? 'Signing in…' : 'Click to sign in'}
                              </span>
                              <span className="text-slate-400 text-xs group-hover:text-white">→</span>
                            </div>
                          </button>
                        );
                      })}
                    </div>
                  </div>
                );
              })}
            </div>

            <div className="mt-8 text-xs font-mono text-slate-500 leading-relaxed">
              Production replaces this with SSO / NextAuth. The dev persona switcher is gated by{' '}
              <code className="text-slate-400">NODE_ENV</code> +{' '}
              <code className="text-slate-400">DEV_ACTOR_PIN</code>.
            </div>
          </>
        )}
      </main>
    </div>
  );
};