'use client';

import React, { useEffect, useMemo, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import { Icon, type IconName } from '@/components/icons';
import { T } from '@/components/i18n/T';
import { UserAvatar, roleGlyph, roleLabel, roleBadge, type StaffLevel } from './UserAvatar';
import { ROLE_RANK, ROLE_LEVEL as ROLE_LEVEL_DISPLAY, type DisplayRoleName } from '@/org/display';
import { deptLabel as deptLabelFn, deptIcon as deptIconFn, deptCode as deptCodeFn } from '@/perm/depts';
import { signOutActor } from '@/app/actions/actor';

type GroupBy = 'department' | 'level' | 'role';
type SortBy = 'level' | 'name' | 'role';
const LS_GROUP = 'folio.persona.groupby';
const LS_SORT = 'folio.persona.sortby';

const GROUP_OPTIONS: { key: GroupBy; id: string; icon: IconName }[] = [
  { key: 'department', id: 'persona.groupDept', icon: 'building' },
  { key: 'level',      id: 'persona.groupLevel', icon: 'gauge' },
  { key: 'role',       id: 'persona.groupRole',  icon: 'shield' },
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
function positionLabel(roleName: string, deptKey: string | null): string {
  const cleanRole = roleName.indexOf('::') >= 0 ? roleName.slice(0, roleName.indexOf('::')) : roleName;
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
    if (!open) {
      setQuery('');
      setHighlight(0);
    }
  }, [open]);

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
          'flex h-10 min-w-0 items-center gap-2 rounded-lg border pl-1.5 pr-2.5 transition-all cursor-pointer',
          open
            ? 'border-accent bg-surface-glass-strong ring-2 ring-accent/20'
            : 'border-glass-border bg-surface-glass-heavy hover:border-glass-border-strong hover:bg-surface-glass-strong',
        ].join(' ')}
      >
        <UserAvatar
          fullname={currentUser?.fullname}
          role={currentUser?.role_id || currentUser?.role_name}
          level={currentUser ? staffLevelOf(currentUser) : undefined}
          size="sm"
        />
        <div className="hidden min-[360px]:block text-left">
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
          <Icon name="chevron-down" size={14} />
        </span>
      </button>


      {open && (
        <>
          <div
            className="fixed inset-0 z-40 bg-slate-950/75 backdrop-blur-sm"
            onClick={() => setOpen(false)}
            aria-hidden
          />
          <div
            role="menu"
            className="absolute right-0 mt-2 w-[30rem] max-w-[94vw] glass-panel-heavy rounded-2xl shadow-modal z-50 animate-fade-scale flex flex-col max-h-[min(78vh,680px)] overflow-hidden"
          >
            {/* ── Header ─────────────────────────────────────────── */}
            <div className="relative px-4 pt-4 pb-3 border-b border-slate-800/80">
              <div className="flex items-center gap-2">
                <span className="inline-flex h-7 w-7 items-center justify-center rounded-lg bg-gradient-to-br from-indigo-500/30 to-purple-500/20 border border-indigo-400/40 text-indigo-200">
                  <Icon name="users" size={14} />
                </span>
                <div className="flex-1 min-w-0">
                  <div className="text-xs font-mono font-bold uppercase tracking-widest text-slate-300 leading-tight">
                    <T id="persona.title" />
                  </div>
                  <div className="text-[10px] font-mono uppercase tracking-wider text-slate-500 mt-0.5 truncate">
                    {currentUser?.fullname || 'Anonymous'}
                  </div>
                </div>
                <span className="inline-flex items-center gap-1 rounded-md border border-slate-700 bg-slate-900/70 px-1.5 py-1 text-[10px] font-mono font-bold text-slate-300">
                  <span className="h-1.5 w-1.5 rounded-full bg-emerald-400 animate-pulse" />
                  {flat.length}/{users.length}
                </span>
                <button
                  type="button"
                  onClick={() => setOpen(false)}
                  aria-label="Close"
                  className="inline-flex h-7 w-7 items-center justify-center rounded-lg border border-slate-800 bg-slate-900/60 text-slate-400 hover:text-white hover:border-slate-700 transition-colors"
                >
                  <Icon name="x" size={12} />
                </button>
              </div>
            </div>

            {/* ── Controls ──────────────────────────────────────── */}
            <div className="px-4 py-3 border-b border-slate-800/80 space-y-2.5">
              <ControlRow labelId="persona.group" icon="filter">
                {GROUP_OPTIONS.map((opt) => {
                  const active = groupBy === opt.key;
                  return (
                    <SegPill
                      key={opt.key}
                      active={active}
                      onClick={() => setGroupBy(opt.key)}
                      label={<T id={opt.id} />}
                      icon={opt.icon}
                    />
                  );
                })}
              </ControlRow>
              <ControlRow labelId="persona.sort" icon="sort">
                {SORT_OPTIONS.map((opt) => {
                  const active = sortBy === opt.key;
                  return (
                    <SegPill
                      key={opt.key}
                      active={active}
                      onClick={() => setSortBy(opt.key)}
                      label={<T id={opt.id} />}
                    />
                  );
                })}
              </ControlRow>
            </div>

            {/* ── Search ────────────────────────────────────────── */}
            <div className="px-4 py-3 border-b border-slate-800/80">
              <label className="relative block">
                <span className="absolute inset-y-0 left-0 flex items-center pl-3 text-slate-500 pointer-events-none">
                  <Icon name="search" size={14} />
                </span>
                <input
                  autoFocus
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder="Search by name, code, or role…"
                  className="w-full pl-9 pr-9 py-2.5 rounded-xl glass-input text-sm text-white placeholder:text-slate-500 focus:outline-none"
                />
                {query && (
                  <button
                    type="button"
                    onClick={() => setQuery('')}
                    aria-label="Clear search"
                    className="absolute inset-y-0 right-0 flex items-center pr-3 text-slate-500 hover:text-white transition-colors"
                  >
                    <Icon name="x" size={12} />
                  </button>
                )}
              </label>
            </div>

            {/* ── List ──────────────────────────────────────────── */}
            <div ref={listRef} className="flex-1 min-h-0 overflow-y-auto px-2 py-2">
              {grouped.length === 0 ? (
                <div className="px-4 py-12 text-center">
                  <div className="mx-auto mb-3 inline-flex h-10 w-10 items-center justify-center rounded-full border border-slate-800 bg-slate-900/60 text-slate-500">
                    <Icon name="search" size={16} />
                  </div>
                  <div className="text-xs text-slate-400 font-mono">
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
                    <div key={gKey} className={gi > 0 ? 'mt-2' : ''}>
                      <div className="mx-2 mb-1.5 px-2 pt-1 pb-1 flex items-center gap-2">
                        <span className="text-sm leading-none opacity-90">{gMeta.icon}</span>
                        <span className="text-[10px] uppercase tracking-widest font-mono font-bold text-slate-300">
                          {gMeta.tone === 'level'
                            ? <T id={`persona.level.${Number(gKey.replace(/^P/, ''))}`} />
                            : <T id={gMeta.label} />}
                        </span>
                        <span className="ml-auto inline-flex items-center gap-1 rounded-md border border-slate-800 bg-slate-900/70 px-1.5 py-0.5 text-[9px] font-mono font-bold text-slate-400">
                          <span className="text-slate-500">{gMeta.code}</span>
                          <span className="text-slate-600">·</span>
                          <span className="text-slate-300">{arr.length}</span>
                        </span>
                      </div>
                      <div className="space-y-1">
                        {arr.map((u: any) => {
                          const idx = flat.findIndex((f) => f.id === u.id);
                          const selected = currentUser?.id === u.id;
                          const focused = idx === highlight;
                          const roleKey = u.role_id || u.role_name;
                          return (
                            <button
                              key={u.id}
                              data-idx={idx}
                              role="menuitem"
                              type="button"
                              onClick={() => selectUser(u)}
                              onMouseEnter={() => setHighlight(idx)}
                              className={[
                                'group relative w-full flex items-center gap-3 pl-3 pr-2.5 py-2 rounded-xl text-left transition-all overflow-hidden',
                                focused
                                  ? 'bg-slate-900/80 ring-1 ring-inset ring-slate-700/80'
                                  : 'ring-1 ring-inset ring-transparent hover:bg-slate-900/50',
                                selected ? 'ring-indigo-400/70' : '',
                              ].join(' ')}
                            >
                              {selected && (
                                <span
                                  aria-hidden
                                  className="absolute left-0 top-1.5 bottom-1.5 w-[3px] rounded-full bg-gradient-to-b from-indigo-400 to-purple-500 shadow-[0_0_8px_rgba(99,102,241,0.6)]"
                                />
                              )}
                              <UserAvatar
                                fullname={u.fullname}
                                role={roleKey}
                                level={staffLevelOf(u)}
                                size="sm"
                                ring={selected}
                              />
                              <div className="flex-1 min-w-0">
                                <div className="flex items-center gap-1.5">
                                  <span className={[
                                    'text-[13px] font-semibold truncate',
                                    selected ? 'text-white' : 'text-slate-100',
                                  ].join(' ')}>
                                    {u.fullname}
                                  </span>
                                  {selected && (
                                    <span className="inline-flex items-center gap-0.5 text-[9px] font-mono font-bold uppercase tracking-wider text-emerald-300">
                                      <Icon name="check" size={9} />
                                    </span>
                                  )}
                                </div>
                                <div className="flex items-center gap-1 mt-1 flex-wrap">
                                  <span
                                    className={[
                                      'px-1.5 py-0.5 rounded text-[9px] font-mono font-bold uppercase border tracking-wider',
                                      roleBadge(roleKey),
                                    ].join(' ')}
                                  >
                                    {positionLabel(roleKey, u.department ?? null)}
                                  </span>
                                  <span className="px-1.5 py-0.5 rounded text-[9px] font-mono font-bold uppercase border border-slate-700 bg-slate-800/60 text-slate-300">
                                    P{staffLevelOf(u)}
                                  </span>
                                  <span className="text-[10px] text-slate-500 font-mono tabular-nums">
                                    {u.employee_code}
                                  </span>
                                </div>
                              </div>
                              <span className="text-base leading-none opacity-50 shrink-0 group-hover:opacity-90 transition-opacity">
                                {roleGlyph(roleKey)}
                              </span>
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
            <div className="px-4 py-2.5 border-t border-slate-800/80 bg-slate-950/40 flex items-center justify-between gap-2">
              <div className="flex items-center gap-1 text-[10px] font-mono text-slate-500">
                <Kbd>↑</Kbd>
                <Kbd>↓</Kbd>
                <span className="ml-0.5 mr-1.5">select</span>
                <Kbd>↵</Kbd>
                <span className="ml-0.5 mr-1.5">confirm</span>
                <Kbd>Esc</Kbd>
                <span className="ml-0.5">close</span>
              </div>
              <div className="flex items-center gap-1.5">
                <span className={[
                  'inline-flex items-center gap-1 rounded-md border px-1.5 py-0.5 text-[9px] font-mono font-bold uppercase tracking-wider',
                  roleBadge(curRole),
                ].join(' ')}>
                  <span className="opacity-70">●</span>
                  {positionLabel(curRole, currentUser?.department ?? null)}
                </span>
                <button
                  type="button"
                  onClick={signOut}
                  className="inline-flex items-center gap-1 rounded-md border border-rose-500/40 bg-rose-500/10 px-1.5 py-0.5 text-[9px] font-mono font-bold uppercase tracking-wider text-rose-200 hover:bg-rose-500/20 hover:border-rose-400/60 transition-colors"
                >
                  <Icon name="arrow-right" size={10} />
                  <T id="chrome.signOut" />
                </button>
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  );
};

const Kbd: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <kbd className="inline-flex min-w-[1.25rem] h-[1.125rem] items-center justify-center rounded border border-slate-700 bg-slate-900/80 px-1 text-[9px] font-mono font-bold text-slate-300 shadow-[inset_0_-1px_0_rgba(0,0,0,0.4)]">
    {children}
  </kbd>
);

const ControlRow: React.FC<{
  labelId: string;
  icon: IconName;
  children: React.ReactNode;
}> = ({ labelId, icon, children }) => (
  <div className="flex items-center gap-2">
    <span className="inline-flex items-center gap-1 text-[10px] font-mono uppercase tracking-widest text-slate-500 shrink-0 w-16">
      <Icon name={icon} size={10} />
      <T id={labelId} />
    </span>
    <div className="flex items-center gap-1 p-0.5 rounded-lg border border-slate-800 bg-slate-950/60 flex-1 min-w-0">
      {children}
    </div>
  </div>
);

const SegPill: React.FC<{
  active: boolean;
  onClick: () => void;
  label: React.ReactNode;
  icon?: IconName;
}> = ({ active, onClick, label, icon }) => (
  <button
    type="button"
    onClick={onClick}
    className={[
      'flex-1 inline-flex items-center justify-center gap-1.5 px-2.5 py-1.5 rounded-md text-[11px] font-mono font-bold uppercase tracking-wider transition-all',
      active
        ? 'bg-gradient-to-b from-indigo-500/40 to-purple-500/30 text-white border border-indigo-400/60 shadow-[inset_0_1px_0_rgba(255,255,255,0.12),0_0_12px_rgba(99,102,241,0.25)]'
        : 'text-slate-400 border border-transparent hover:text-slate-100 hover:bg-slate-900/60',
    ].join(' ')}
  >
    {icon && <Icon name={icon} size={11} />}
    {label}
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