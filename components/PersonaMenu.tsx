'use client';

import React, { useEffect, useMemo, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import {
  ArrowDownUp,
  ArrowRight,
  Building2,
  Check,
  ChevronDown,
  Filter,
  Gauge,
  Search,
  Shield,
  Users,
  X,
  type LucideIcon,
} from 'lucide-react';
import { T } from '@/components/i18n/T';
import { UserAvatar, roleGlyph, roleLabel, roleBadge, type StaffLevel } from './UserAvatar';
import { ROLE_RANK, ROLE_LEVEL as ROLE_LEVEL_DISPLAY, type DisplayRoleName } from '@/org/display';
import { deptLabel as deptLabelFn, deptIcon as deptIconFn, deptCode as deptCodeFn } from '@/perm/depts';
import { signOutActor } from '@/app/actions/actor';

type GroupBy = 'department' | 'level' | 'role';
type SortBy = 'level' | 'name' | 'role';
const LS_GROUP = 'folio.persona.groupby';
const LS_SORT = 'folio.persona.sortby';

const GROUP_OPTIONS: { key: GroupBy; id: string; icon: LucideIcon }[] = [
  { key: 'department', id: 'persona.groupDept', icon: Building2 },
  { key: 'level',      id: 'persona.groupLevel', icon: Gauge },
  { key: 'role',       id: 'persona.groupRole',  icon: Shield },
];
const SORT_OPTIONS: { key: SortBy; id: string }[] = [
  { key: 'level', id: 'persona.sortLevel' },
  { key: 'name',  id: 'persona.sortName' },
  { key: 'role',  id: 'persona.sortRole' },
];

function lsGet<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  if (typeof window === 'undefined') return fallback;
  const v = window.localStorage.getItem(key);
  return v && (allowed as readonly string[]).includes(v) ? (v as T) : fallback;
}
function lsSet(key: string, v: string) {
  if (typeof window === 'undefined') return;
  try { window.localStorage.setItem(key, v); } catch {}
}

const GENERIC_ROLES = new Set(['supervisor', 'officer', 'manager', 'staff', 'head_of_department']);
function positionLabel(roleName: string | null | undefined, deptKey: string | null): string {
  const role = roleName || 'unconfigured';
  const cleanRole = role.indexOf('::') >= 0 ? role.slice(0, role.indexOf('::')) : role;
  const r = roleLabel(cleanRole);
  if (deptKey && GENERIC_ROLES.has(cleanRole)) {
    const d = deptLabelFn((deptKey || '').replace(/^dept-/, ''));
    return `${d} ${r}`;
  }
  return r;
}

function staffLevelOf(u: any): StaffLevel {
  const lv = u.level;
  if (lv === 1 || lv === 2 || lv === 3 || lv === 4 || lv === 5) return lv;
  const role = u.role_id || u.role_name || '';
  return ROLE_LEVEL_DISPLAY[role as DisplayRoleName] ?? 5;
}

interface PersonaMenuProps {
  users: any[];
  currentUser: any;
}

export const PersonaMenu: React.FC<PersonaMenuProps> = ({ users, currentUser }) => {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [highlight, setHighlight] = useState(0);
  const [groupBy, setGroupBy] = useState<GroupBy>(() => lsGet(LS_GROUP, ['department', 'level', 'role'] as const, 'department'));
  const [sortBy, setSortBy] = useState<SortBy>(() => lsGet(LS_SORT, ['level', 'name', 'role'] as const, 'level'));
  const buttonRef = useRef<HTMLButtonElement | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => { lsSet(LS_GROUP, groupBy); }, [groupBy]);
  useEffect(() => { lsSet(LS_SORT, sortBy); }, [sortBy]);

  useEffect(() => {
    if (!open) {
      setQuery('');
      setHighlight(0);
    }
  }, [open]);

  const grouped = useMemo(() => {
    const q = query.trim().toLowerCase();
    const filtered = !q
      ? users
      : users.filter(
          (u) =>
            u.fullname?.toLowerCase().includes(q) ||
            u.employee_code?.toLowerCase().includes(q) ||
            u.role_name?.toLowerCase().includes(q) ||
            u.role_id?.toLowerCase().includes(q) ||
            u.dept_group_name?.toLowerCase().includes(q) ||
            u.department?.toLowerCase().includes(q),
        );

    const comparator = (a: any, b: any) => {
      if (sortBy === 'name') return (a.fullname || '').localeCompare(b.fullname || '');
      if (sortBy === 'role') {
        const ra = ROLE_RANK[a.role_id as DisplayRoleName] ?? 99;
        const rb = ROLE_RANK[b.role_id as DisplayRoleName] ?? 99;
        return ra !== rb ? ra - rb : (a.fullname || '').localeCompare(b.fullname || '');
      }
      return (
        staffLevelOf(a) - staffLevelOf(b) ||
        (ROLE_RANK[a.role_id as DisplayRoleName] ?? 99) - (ROLE_RANK[b.role_id as DisplayRoleName] ?? 99) ||
        (a.fullname || '').localeCompare(b.fullname || '')
      );
    };

    const map = new Map<string, any[]>();
    for (const u of filtered) {
      let key: string;
      if (groupBy === 'department') {
        key = u.dept_id || u.dept_group_name || u.department || '__other__';
      } else if (groupBy === 'level') {
        key = `P${staffLevelOf(u)}`;
      } else {
        key = u.role_id || u.role_name || '__other__';
      }
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(u);
    }
    for (const [, arr] of map) arr.sort(comparator);

    const entries = [...map.entries()];
    entries.sort((a, b) => {
      if (groupBy === 'level') {
        return a[0].localeCompare(b[0], undefined, { numeric: true });
      }
      if (groupBy === 'role') {
        const ra = ROLE_RANK[(a[1][0]?.role_id) as DisplayRoleName] ?? 99;
        const rb = ROLE_RANK[(b[1][0]?.role_id) as DisplayRoleName] ?? 99;
        if (ra !== rb) return ra - rb;
      }
      if (b[1].length !== a[1].length) return b[1].length - a[1].length;
      const na = a[1][0]?.dept_group_name || a[1][0]?.department || a[0];
      const nb = b[1][0]?.dept_group_name || b[1][0]?.department || b[0];
      return na.localeCompare(nb);
    });
    return entries;
  }, [users, query, groupBy, sortBy]);

  const flat = useMemo(() => grouped.flatMap(([, arr]) => arr), [grouped]);

  const selectUser = async (u: any) => {
    setOpen(false);
    try {
      await fetch('/api/actor', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ id: u.id }),
      });
    } catch {}
    router.refresh();
  };

  const signOut = async () => {
    setOpen(false);
    await signOutActor();
  };

  useEffect(() => {
    function onExternal() { setOpen(true); }
    window.addEventListener('folio:open-persona', onExternal);
    return () => window.removeEventListener('folio:open-persona', onExternal);
  }, []);

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setOpen(false);
        buttonRef.current?.focus();
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        setHighlight((h) => Math.min(h + 1, flat.length - 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setHighlight((h) => Math.max(h - 1, 0));
      } else if (e.key === 'Enter') {
        const u = flat[highlight];
        if (u) selectUser(u);
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, flat.length, highlight]);

  useEffect(() => {
    if (open && flat.length) setHighlight(0);
  }, [query, open, flat.length]);

  useEffect(() => {
    if (!open || !listRef.current) return;
    const el = listRef.current.querySelector<HTMLElement>(`[data-idx="${highlight}"]`);
    if (el) el.scrollIntoView({ block: 'nearest' });
  }, [highlight, open]);

  const curRole = currentUser?.role_id || currentUser?.role_name;

  return (
    <div className="relative" ref={rootRef}>
      <button
        ref={buttonRef}
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="menu"
        aria-expanded={open}
        className={[
          'flex h-10 min-w-0 items-center gap-1.5 rounded-lg border pl-1.5 pr-1.5 transition-all cursor-pointer sm:gap-2 sm:pr-2.5',
          open
            ? 'border-accent bg-paper-3 ring-2 ring-accent/20'
            : 'border-rule bg-paper-2 hover:border-rule-strong hover:bg-paper-3',
        ].join(' ')}
      >
        <UserAvatar
          fullname={currentUser?.fullname}
          role={currentUser?.role_id || currentUser?.role_name}
          level={currentUser ? staffLevelOf(currentUser) : undefined}
          size="sm"
        />
        <div className="hidden text-left sm:block">
          <span className="block text-xs font-semibold text-ink leading-tight">
            {currentUser?.fullname?.split(' ')[0] || 'User'}
          </span>
          <span
            className={[
              'inline-block rounded-md border px-1.5 py-0.5 text-[9px] font-mono font-bold uppercase leading-none mt-0.5',
              roleBadge(curRole),
            ].join(' ')}
          >
            {roleLabel(curRole)}
          </span>
        </div>
        <span
          className="ml-0.5 text-mute transition-transform duration-200"
          style={{ transform: open ? 'rotate(180deg)' : 'rotate(0deg)' }}
          aria-hidden
        >
          <ChevronDown size={14} />
        </span>
      </button>


      {open && (
        <>
          <div
            className="fixed inset-0 z-sticky bg-paper-2/75 backdrop-blur-sm"
            onClick={() => setOpen(false)}
            aria-hidden
          />
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="persona-menu-title"
            className="fixed left-3 right-3 top-16 z-fixed mt-0 flex max-h-[calc(100dvh-10.5rem)] w-auto max-w-none animate-fade-scale flex-col overflow-hidden rounded-2xl border border-rule-strong bg-paper-2 shadow-modal sm:absolute sm:left-auto sm:right-0 sm:top-auto sm:mt-2 sm:max-h-[min(88dvh,46rem)] sm:w-[34rem] sm:max-w-[calc(100vw-1.5rem)]"
          >
            {/* ── Header ─────────────────────────────────────────── */}
            <div className="relative border-b border-rule/80 px-4 py-4 sm:px-5">
              <div className="flex items-center gap-3">
                <span className="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-xl border border-positive/40 bg-positive-soft text-positive">
                  <Users size={18} />
                </span>
                <div className="min-w-0 flex-1">
                  <h2 id="persona-menu-title" className="text-base font-semibold leading-tight text-ink">
                    <T
                      id="persona.title"
                      variant="stacked"
                      primaryClassName="block text-base font-semibold text-ink"
                      secondaryClassName="mt-0.5 block text-xs font-normal text-mute"
                    />
                  </h2>
                  <p className="mt-1 truncate text-xs text-ink-2">
                    {currentUser?.fullname || 'Anonymous'}
                  </p>
                </div>
                <span className="inline-flex h-9 shrink-0 items-center gap-1.5 rounded-full border border-rule bg-paper-3 px-3 text-xs font-mono font-semibold tabular-nums text-ink-2">
                  <span className="h-1.5 w-1.5 rounded-full bg-positive" />
                  {flat.length}/{users.length}
                </span>
                <button
                  type="button"
                  onClick={() => setOpen(false)}
                  aria-label="Close"
                  className="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-rule bg-paper-2 text-ink-2 transition-colors hover:border-rule-strong hover:bg-paper-3 hover:text-ink"
                >
                  <X size={16} />
                </button>
              </div>
            </div>

            {/* ── Controls ──────────────────────────────────────── */}
            <div className="space-y-3 border-b border-rule/80 px-4 py-3 sm:px-5">
              <ControlRow labelId="persona.group" icon={Filter}>
                {GROUP_OPTIONS.map((opt) => {
                  const active = groupBy === opt.key;
                  return (
                    <SegPill
                      key={opt.key}
                      active={active}
                      onClick={() => setGroupBy(opt.key)}
                      label={(
                        <T
                          id={opt.id}
                          variant="compact"
                          primaryClassName="font-medium text-current"
                          secondaryClassName="ml-1 text-[10px] font-normal text-current opacity-75"
                        />
                      )}
                      icon={opt.icon}
                    />
                  );
                })}
              </ControlRow>
              <ControlRow labelId="persona.sort" icon={ArrowDownUp}>
                {SORT_OPTIONS.map((opt) => {
                  const active = sortBy === opt.key;
                  return (
                    <SegPill
                      key={opt.key}
                      active={active}
                      onClick={() => setSortBy(opt.key)}
                      label={(
                        <T
                          id={opt.id}
                          variant="compact"
                          primaryClassName="font-medium text-current"
                          secondaryClassName="ml-1 text-[10px] font-normal text-current opacity-75"
                        />
                      )}
                    />
                  );
                })}
              </ControlRow>
            </div>

            {/* ── Search ────────────────────────────────────────── */}
            <div className="border-b border-rule/80 px-4 py-3 sm:px-5">
              <label className="relative block">
                <span className="pointer-events-none absolute inset-y-0 left-0 flex items-center pl-3.5 text-mute">
                  <Search size={16} />
                </span>
                <input
                  autoFocus
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder="Search by name, code, or role…"
                  className="h-11 w-full rounded-xl border border-rule bg-paper-1 pl-10 pr-10 text-sm text-ink outline-none transition placeholder:text-mute focus:border-accent focus:ring-2 focus:ring-accent/20"
                />
                {query && (
                  <button
                    type="button"
                    onClick={() => setQuery('')}
                    aria-label="Clear search"
                    className="absolute inset-y-0 right-0 flex items-center pr-3.5 text-mute transition-colors hover:text-ink"
                  >
                    <X size={14} />
                  </button>
                )}
              </label>
            </div>

            {/* ── List ──────────────────────────────────────────── */}
            <div
              ref={listRef}
              role="listbox"
              aria-label="Available user roles"
              className="min-h-0 flex-1 overflow-y-auto px-2 py-2"
            >
              {grouped.length === 0 ? (
                <div className="px-4 py-12 text-center">
                  <div className="mx-auto mb-3 inline-flex h-10 w-10 items-center justify-center rounded-full border border-rule bg-paper-2/60 text-mute">
                    <Search size={16} />
                  </div>
                  <div className="text-xs text-ink-2 font-mono">
                    <T id="persona.noMatch" values={{ query }} />
                  </div>
                </div>
              ) : (
                grouped.map(([gKey, arr], gi) => {
                  const head = arr[0];
                  const gMeta = (() => {
                    if (groupBy === 'level') {
                      const lv = Number(gKey.replace(/^P/, '')) as StaffLevel;
                      return {
                        icon: STAFF_LEVEL_GLYPH[lv] ?? '🧭',
                        label: `P${lv}`,
                        code: gKey,
                        tone: 'level',
                      };
                    }
                    if (groupBy === 'role') {
                      const rk = (head?.role_id || head?.role_name || gKey) as DisplayRoleName;
                      return {
                        icon: roleGlyph(rk as any) || '🛡️',
                        label: roleLabel(rk as any) || gKey,
                        code: (head?.role_id || gKey).toString().toUpperCase().slice(0, 6),
                        tone: 'role',
                      };
                    }
                    const dKey = gKey !== '__other__' ? gKey : (head?.department || 'department');
                    const m = deptMetaFromKey(dKey);
                    return { icon: m.icon, label: m.label, code: m.code, tone: 'dept' };
                  })();
                  return (
                    <div key={gKey} className={gi > 0 ? 'mt-3' : ''}>
                      <div className="sticky top-0 z-10 mx-1 mb-1.5 flex items-center gap-2 rounded-lg bg-paper-2/95 px-3 py-2 backdrop-blur-md">
                        <span className="text-sm leading-none" aria-hidden>{gMeta.icon}</span>
                        <span className="text-xs font-semibold text-ink-2">
                          {gMeta.tone === 'level'
                            ? <T id={`persona.level.${Number(gKey.replace(/^P/, ''))}`} />
                            : <T id={gMeta.label} />}
                        </span>
                        <span className="ml-auto inline-flex items-center gap-1.5 rounded-full border border-rule bg-paper-3 px-2 py-1 text-[10px] font-mono font-semibold tabular-nums text-ink-2">
                          <span>{gMeta.code}</span>
                          <span className="text-mute">{arr.length}</span>
                        </span>
                      </div>
                      <div className="space-y-1.5">
                        {arr.map((u: any) => {
                          const idx = flat.findIndex((f) => f.id === u.id);
                          const selected = currentUser?.id === u.id;
                          const focused = idx === highlight;
                          const roleKey = u.role_id || u.role_name;
                          return (
                            <button
                              key={u.id}
                              data-idx={idx}
                              role="option"
                              aria-selected={selected}
                              type="button"
                              onClick={() => selectUser(u)}
                              onMouseEnter={() => setHighlight(idx)}
                              className={[
                                'group relative flex min-h-[4.25rem] w-full items-center gap-3 overflow-hidden rounded-xl border px-3 py-2.5 text-left transition-all',
                                focused
                                  ? 'border-rule-strong bg-paper-3'
                                  : 'border-transparent hover:border-rule hover:bg-paper-3/60',
                                selected ? 'border-positive/50 bg-positive-soft/40' : '',
                              ].join(' ')}
                            >
                              {selected && (
                                <span
                                  aria-hidden
                                  className="absolute inset-y-2 left-0 w-0.5 rounded-full bg-positive"
                                />
                              )}
                              <UserAvatar
                                fullname={u.fullname}
                                role={roleKey}
                                level={staffLevelOf(u)}
                                size="sm"
                                ring={selected}
                                className="border border-rule-strong bg-paper-3"
                              />
                              <div className="flex-1 min-w-0">
                                <span className="block truncate text-sm font-semibold text-ink">
                                  {u.fullname}
                                </span>
                                <div className="mt-1 flex min-w-0 items-center gap-1.5 text-xs text-mute">
                                  <span className="truncate text-ink-2">{positionLabel(roleKey, u.department ?? null)}</span>
                                  <span aria-hidden>·</span>
                                  <span className="shrink-0 font-mono tabular-nums">P{staffLevelOf(u)}</span>
                                  {u.employee_code && (
                                    <>
                                      <span aria-hidden>·</span>
                                      <span className="shrink-0 font-mono tabular-nums">{u.employee_code}</span>
                                    </>
                                  )}
                                </div>
                              </div>
                              {selected && (
                                <span className="grid h-7 w-7 shrink-0 place-items-center rounded-full bg-positive text-paper" aria-label="Current role">
                                  <Check size={14} strokeWidth={3} />
                                </span>
                              )}
                            </button>
                          );
                        })}
                      </div>
                    </div>
                  );
                })
              )}
            </div>

            {/* ── Footer ────────────────────────────────────────── */}
            <div className="flex items-center justify-between gap-3 border-t border-rule/80 bg-paper-1/50 px-4 py-3 sm:px-5">
              <p className="hidden min-w-0 truncate text-xs text-mute sm:block">
                <T id="persona.filterHint" hideSecondary />
              </p>
              <button
                type="button"
                onClick={signOut}
                className="ml-auto inline-flex min-h-9 shrink-0 items-center gap-2 rounded-lg border border-critical/50 bg-critical-soft px-3 py-2 text-xs font-semibold text-critical transition-colors hover:border-critical hover:bg-critical/15"
              >
                <ArrowRight size={13} />
                <T
                  id="chrome.signOut"
                  variant="compact"
                  primaryClassName="font-semibold text-critical"
                  secondaryClassName="ml-1 text-[11px] font-normal text-critical/80"
                />
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  );
};

const ControlRow: React.FC<{
  labelId: string;
  icon: LucideIcon;
  children: React.ReactNode;
}> = ({ labelId, icon: IconCmp, children }) => (
  <fieldset className="min-w-0 space-y-1.5">
    <legend className="flex items-center gap-1.5 text-xs font-medium text-ink-2">
      <IconCmp size={13} aria-hidden />
      <T id={labelId} variant="compact" />
    </legend>
    <div className="grid min-w-0 grid-cols-3 gap-1 rounded-xl border border-rule bg-paper-1 p-1">
      {children}
    </div>
  </fieldset>
);

const SegPill: React.FC<{
  active: boolean;
  onClick: () => void;
  label: React.ReactNode;
  icon?: LucideIcon;
}> = ({ active, onClick, label, icon: IconCmp }) => (
  <button
    type="button"
    onClick={onClick}
    aria-pressed={active}
    className={[
      'inline-flex min-h-9 min-w-0 items-center justify-center gap-1.5 rounded-lg border px-2 py-1.5 text-xs font-medium transition-all',
      active
        ? 'border-accent/60 bg-accent-soft text-accent-ink shadow-sm'
        : 'border-transparent text-ink-2 hover:border-rule hover:bg-paper-3 hover:text-ink',
    ].join(' ')}
  >
    {IconCmp && <IconCmp className="shrink-0" size={13} aria-hidden />}
    <span className="min-w-0 truncate">{label}</span>
  </button>
);

const STAFF_LEVEL_GLYPH: Record<number, string> = {
  1: '👑',
  2: '💼',
  3: '🛡️',
  4: '👥',
  5: '👤',
};

function deptMetaFromKey(key: string): { code: string; icon: string; label: string; key: string } {
  const cleaned = (key || '').replace(/^dept-/, '');
  return {
    key: cleaned,
    code: deptCodeFn(cleaned),
    icon: deptIconFn(cleaned),
    label: deptLabelFn(cleaned),
  };
}
