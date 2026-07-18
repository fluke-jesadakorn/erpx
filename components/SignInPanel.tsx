'use client';

import React, { useEffect, useState } from 'react';
import { useRouter } from 'next/navigation';
import { useToast } from '@/components/ui/Toast';
import { T } from '@/components/i18n/T';
import { ROLE_RANK, ROLE_LEVEL, type DisplayRoleName, type StaffLevel } from '@/org/display';
import { useTranslations } from 'next-intl';
import {
  ArrowRight,
  Crown,
  LoaderCircle,
  Search,
  ShieldCheck,
  Telescope,
  UserRound,
  UsersRound,
  type LucideIcon,
} from 'lucide-react';
import { Input } from '@/components/ui/Input';

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
  staff:               'from-positive via-positive-strong to-positive-strong border-positive',
  officer:             'from-positive via-positive-strong to-positive-strong border-positive',
  sales_rep:           'from-positive via-positive-strong to-positive-strong border-positive',
  accountant:          'from-info via-info-strong to-info-strong border-info',
  account_officer:     'from-info via-info-strong to-info-strong border-info',
  account_supervisor:  'from-info via-info-strong to-info-strong border-info',
  accounting_manager:  'from-info via-info-strong to-info-strong border-info',
  supervisor:          'from-caution via-caution-strong to-caution-strong border-caution',
  sales_supervisor:    'from-info via-info-strong to-info-strong border-info',
  head_of_department:  'from-caution via-caution-strong to-caution-strong border-caution',
  manager:             'from-caution via-caution-strong to-caution-strong border-caution',
  admin:               'from-accent via-accent-strong to-accent-strong border-accent',
  cfo:                 'from-accent via-accent-strong to-accent-strong border-accent',
  finance:             'from-accent via-accent-strong to-accent-strong border-accent',
  ceo:                 'from-critical via-critical-strong to-critical-strong border-critical',
  it:                  'from-paper-2/25 via-paper-3/15 to-paper/40 border-rule/40',
  hr:                  'from-accent via-accent-strong to-accent-strong border-accent',
  hr_manager:          'from-accent via-accent-strong to-accent-strong border-accent',
  it_manager:          'from-info via-info-strong to-info-strong border-info',
  it_supervisor:       'from-info via-info-strong to-info-strong border-info',
  it_officer:          'from-info via-info-strong to-info-strong border-info',
  hr_supervisor:       'from-accent via-accent-strong to-accent-strong border-accent',
  hr_officer:          'from-accent via-accent-strong to-accent-strong border-accent',
  accounting_supervisor: 'from-info via-info-strong to-info-strong border-info',
  accounting_officer:  'from-info via-info-strong to-info-strong border-info',
  finance_manager:     'from-accent via-accent-strong to-accent-strong border-accent',
  finance_supervisor:  'from-accent via-accent-strong to-accent-strong border-accent',
  finance_officer:     'from-accent via-accent-strong to-accent-strong border-accent',
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
  it_manager: 'IT Manager',
  it_supervisor: 'IT Supervisor',
  it_officer: 'IT Officer',
  hr_supervisor: 'HR Supervisor',
  hr_officer: 'HR Officer',
  accounting_supervisor: 'Accounting Supervisor',
  accounting_officer: 'Accounting Officer',
  finance_manager: 'Financial Manager',
  finance_supervisor: 'Financial Supervisor',
  finance_officer: 'Financial Officer',
};

const LEVEL_ORDER: StaffLevel[] = [1, 2, 3, 4, 5];

const LEVEL_GLYPH: Record<StaffLevel, LucideIcon> = {
  1: Crown,
  2: ShieldCheck,
  3: Telescope,
  4: UsersRound,
  5: UserRound,
};

const LEVEL_ACCENT: Record<StaffLevel, string> = {
  1: 'bg-critical-soft text-critical-strong border border-critical border-critical',
  2: 'bg-accent text-paper border-accent',
  3: 'bg-caution-soft text-caution-strong border border-caution border-caution',
  4: 'bg-info text-paper border-info',
  5: 'bg-positive text-paper border-positive',
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
  const t = useTranslations();
  const [users, setUsers] = useState<SeedUser[]>([]);
  const [pinRequired, setPinRequired] = useState(false);
  const [pin, setPin] = useState('');
  const [busyId, setBusyId] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState('');

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
      const haystack = [u.fullname, u.employee_code, u.role_name, ROLE_LABEL[u.role_id], u.department, u.dept_group_name]
        .filter(Boolean)
        .join(' ')
        .toLocaleLowerCase();
      if (query.trim() && !haystack.includes(query.trim().toLocaleLowerCase())) continue;
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
  }, [query, users]);

  async function signIn(user: SeedUser) {
    if (pinRequired && !pin.trim()) {
      toast.error(t('signIn.pinRequired'));
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
        toast.error(data?.error || t('signIn.failedWith', { status: res.status }));
        return;
      }
      toast.success(t('signIn.signedIn', { name: user.fullname }), data.role);
      router.refresh();
    } catch (e: any) {
      toast.error(e?.message || t('signIn.failed'));
    } finally {
      setBusyId(null);
    }
  }

  return (
    <div className="app-shell relative min-h-screen overflow-hidden px-4 py-6 text-ink selection:bg-accent selection:text-ink sm:px-6 sm:py-10">
      <div aria-hidden className="absolute left-[8%] top-[-14rem] h-[34rem] w-[34rem] rounded-full bg-accent-soft/50 blur-3xl" />
      <div aria-hidden className="absolute bottom-[-16rem] right-[4%] h-[32rem] w-[32rem] rounded-full bg-info-soft/35 blur-3xl" />
      <main className="relative z-10 mx-auto w-full max-w-6xl">
        <section className="panel-elevated grid min-h-[calc(100vh-5rem)] overflow-hidden lg:grid-cols-[0.78fr_1.22fr]">
          <div className="relative flex min-h-[18rem] flex-col justify-between overflow-hidden border-b border-rule p-6 sm:p-8 lg:min-h-0 lg:border-b-0 lg:border-r">
            <div aria-hidden className="absolute inset-0 bg-[linear-gradient(color-mix(in_oklab,var(--rule)_28%,transparent)_1px,transparent_1px),linear-gradient(90deg,color-mix(in_oklab,var(--rule)_28%,transparent)_1px,transparent_1px)] bg-[size:34px_34px] [mask-image:linear-gradient(to_bottom,black,transparent)]" />
            <div className="relative">
              <div className="inline-flex items-center gap-2.5">
                <span className="flex h-10 w-10 items-center justify-center rounded-2xl border border-accent/35 bg-accent-soft/70 font-display text-sm font-semibold text-accent shadow-inner">F</span>
                <span className="font-display text-xl font-semibold tracking-[-0.04em]">Folio</span>
              </div>
            </div>
            <div className="relative max-w-md">
              <span className="glass-chip inline-flex px-2.5 py-1 text-[10px] font-medium uppercase tracking-[0.15em] text-accent">Executive crystal workspace</span>
              <h1 className="mt-4 font-display text-3xl font-semibold leading-tight tracking-[-0.04em] sm:text-4xl">Choose your secure workspace.</h1>
              <p className="mt-3 text-sm leading-relaxed text-ink-2"><T id="signIn.subtitle" hideSecondary /></p>
            </div>
            <p className="relative mt-6 max-w-sm text-xs leading-relaxed text-mute"><T id="signIn.productionNote" hideSecondary /></p>
          </div>

          <div className="flex min-w-0 flex-col p-4 sm:p-6 lg:p-8">
            <div className="mb-5 flex flex-wrap items-end justify-between gap-3">
              <div>
                <p className="text-[10px] font-medium uppercase tracking-[0.15em] text-accent">Identity access</p>
                <h2 className="mt-1 font-display text-xl font-semibold tracking-tight">Select persona</h2>
              </div>
              {!loading && !error && <span className="glass-chip px-2.5 py-1 text-xs text-mute">{users.length} profiles</span>}
            </div>

        {loading && (
          <div className="panel flex items-center justify-center gap-2 p-10 text-sm text-ink-2"><LoaderCircle size={16} className="animate-spin" aria-hidden /><T id="signIn.loadingUsers" hideSecondary /></div>
        )}
        {error && (
          <div className="panel border-critical/50 p-6">
            <div className="text-sm text-critical font-mono">
              <T id="signIn.loadFailed" hideSecondary values={{ error }} />
            </div>
            <p className="text-xs text-mute mt-2">
              <T id="signIn.runSeedHint" hideSecondary />
            </p>
          </div>
        )}

        {!loading && !error && (
          <>
            <div className="relative mb-4">
              <Search size={16} aria-hidden className="pointer-events-none absolute left-3.5 top-1/2 z-10 -translate-y-1/2 text-mute" />
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search name, role, department, or code"
                aria-label="Search personas"
                className="pl-10"
              />
            </div>
            {pinRequired && (
              <div className="panel mb-4 border-caution/45 p-4">
                <label className="block">
                  <span className="text-xs font-mono uppercase tracking-wider text-ink-2">
                    <T id="signIn.devActorPin" hideSecondary />
                  </span>
                  <input
                    type="password"
                    value={pin}
                    onChange={(e) => setPin(e.target.value)}
                    placeholder={t('signIn.pinPlaceholder')}
                    className="glass-input mt-2 w-full px-3 py-2 text-[13px] text-ink placeholder:text-mute"
                  />
                </label>
              </div>
            )}

            <div className="max-h-[58vh] space-y-5 overflow-y-auto pr-1 lg:max-h-[calc(100vh-20rem)]">
              {LEVEL_ORDER.map((lv) => {
                const groupUsers = levelBuckets[lv];
                if (groupUsers.length === 0) return null;
                const LevelIcon = LEVEL_GLYPH[lv];
                return (
                  <div key={lv} className="space-y-2.5">
                    <div className={`inline-flex items-center gap-2 rounded-lg border px-2.5 py-1 text-[10px] font-medium uppercase tracking-[0.13em] ${LEVEL_ACCENT[lv]}`}>
                      <LevelIcon size={12} aria-hidden />
                      <span><T id={`persona.level.${lv}`} hideSecondary /></span>
                    </div>
                    <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                      {groupUsers.map((u) => {
                        const tone = ROLE_ACCENT[u.role_id] || ROLE_ACCENT.staff;
                        return (
                          <button
                            key={u.id}
                            type="button"
                            disabled={busyId !== null}
                            onClick={() => signIn(u)}
                            className={`panel-interactive group relative min-w-0 rounded-xl border p-3 text-left disabled:cursor-wait disabled:opacity-50 ${tone}`}
                          >
                            <div className="flex items-start justify-between gap-2">
                              <div className="min-w-0">
                                <div className="text-base font-black text-ink truncate">{u.fullname}</div>
                                <div className="text-xs font-mono text-ink-2 mt-0.5">
                                  {u.employee_code} · {u.dept_group_name ?? u.department ?? '—'}
                                </div>
                              </div>
                              <span className="text-xs font-mono uppercase tracking-wider px-1.5 py-0.5 rounded-full bg-paper-2/60 border border-rule text-ink shrink-0">
                                {ROLE_LABEL[u.role_id] || u.role_name}
                              </span>
                            </div>
                            <div className="mt-3 flex items-center justify-between">
                              <span className="text-xs text-mute font-mono">
                                {busyId === u.id ? <T id="signIn.signingIn" hideSecondary /> : <T id="signIn.clickToSignIn" hideSecondary />}
                              </span>
                              {busyId === u.id ? <LoaderCircle size={14} className="animate-spin text-accent" aria-hidden /> : <ArrowRight size={14} className="text-mute transition-transform group-hover:translate-x-px group-hover:text-ink" aria-hidden />}
                            </div>
                          </button>
                        );
                      })}
                    </div>
                  </div>
                );
              })}
            </div>

            {LEVEL_ORDER.every((lv) => levelBuckets[lv].length === 0) && (
              <div className="panel p-8 text-center text-sm text-mute">No personas match “{query}”.</div>
            )}
          </>
        )}
          </div>
        </section>
      </main>
    </div>
  );
};
