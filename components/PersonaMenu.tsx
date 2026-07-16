'use client';

import React, { useEffect, useMemo, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import { Icon } from '@/components/icons';
import { T } from '@/components/i18n/T';
import { UserAvatar, roleGlyph, roleLabel, roleBadge, type StaffLevel } from './UserAvatar';
import { ROLE_RANK, ROLE_LEVEL as ROLE_LEVEL_DISPLAY, type DisplayRoleName } from '@/org/display';
import { deptLabel as deptLabelFn, deptIcon as deptIconFn, deptCode as deptCodeFn } from '@/perm/depts';
import { signOutActor } from '@/app/actions/actor';

type GroupBy = 'department' | 'level' | 'role';
type SortBy = 'level' | 'name' | 'role';
const LS_GROUP = 'folio.persona.groupby';
const LS_SORT = 'folio.persona.sortby';

const GROUP_OPTIONS: { key: GroupBy; id: string; icon: string }[] = [
  { key: 'department', id: 'persona.groupDept', icon: '🏢' },
  { key: 'level',      id: 'persona.groupLevel', icon: '🧭' },
  { key: 'role',       id: 'persona.groupRole',  icon: ' badge' },
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

function initials(name?: string): string {
  if (!name) return '?';
  const p = name.trim().split(/\s+/);
  if (p.length === 1) return p[0].slice(0, 2).toUpperCase();
  return (p[0][0] + p[p.length - 1][0]).toUpperCase();
}

function deptMeta(deptId: string | null | undefined, deptName: string | null | undefined) {
  const cleaned = (deptId || '').replace(/^dept-/, '');
  if (cleaned) {
    return {
      code: deptCodeFn(cleaned),
      icon: deptIconFn(cleaned),
    };
  }
  if (deptName) {
    const code = deptName.split(/\s+/).map((w) => w[0] || '').join('').slice(0, 3).toUpperCase() || 'DEP';
    return { code, icon: '🏢' };
  }
  return { code: 'DEP', icon: '🏢' };
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
            className="absolute right-0 mt-2 w-[28rem] max-w-[92vw] bg-slate-950/98 border border-indigo-500/40 rounded-2xl shadow-2xl shadow-black/70 p-2 z-50 animate-fade-in flex flex-col max-h-[min(72vh,640px)]"
          >
            <div className="px-3 pt-2 pb-1 text-xs font-mono font-bold uppercase tracking-wider text-slate-400 flex items-center justify-between">
              <span><T id="persona.title" /></span>
              <span className="text-slate-500 normal-case font-normal">{flat.length}/{users.length}</span>
            </div>
            <div className="px-1 pt-1 pb-1.5 flex flex-wrap items-center gap-1.5">
              <div className="flex items-center rounded-lg border border-slate-800 bg-slate-950/60 p-0.5">
                {GROUP_OPTIONS.map((opt) => (
                  <button
                    key={opt.key}
                    type="button"
                    onClick={() => setGroupBy(opt.key)}
                    className={[
                      'px-2 py-1 rounded-md text-xs font-mono font-bold uppercase tracking-wider transition-colors',
                      groupBy === opt.key
                        ? 'bg-indigo-500/30 text-indigo-200 border border-indigo-500/50'
                        : 'text-slate-400 hover:text-slate-200 border border-transparent',
                    ].join(' ')}
                    title={`Group by `}
                  >
                    {opt.icon} <T id={opt.id} />
                  </button>
                ))}
              </div>
              <div className="flex items-center rounded-lg border border-slate-800 bg-slate-950/60 p-0.5">
                <span className="px-1.5 text-xs font-mono uppercase text-slate-500"><T id="persona.sort" /></span>
                {SORT_OPTIONS.map((opt) => (
                  <button
                    key={opt.key}
                    type="button"
                    onClick={() => setSortBy(opt.key)}
                    className={[
                      'px-2 py-1 rounded-md text-xs font-mono font-bold uppercase tracking-wider transition-colors',
                      sortBy === opt.key
                        ? 'bg-purple-500/30 text-purple-200 border border-purple-500/50'
                        : 'text-slate-400 hover:text-slate-200 border border-transparent',
                    ].join(' ')}
                    title={`Sort by `}
                  >
                    <T id={opt.id} />
                  </button>
                ))}
              </div>
            </div>
            <div className="px-1 pt-0.5 pb-2">
              <input
                autoFocus
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search by name, code, or role…"
                className="w-full px-3 py-2 rounded-xl bg-slate-950/80 border border-slate-800 text-xs text-white placeholder:text-slate-500 focus:outline-none focus:ring-2 focus:ring-indigo-500/30 focus:border-indigo-500/60"
              />
            </div>

            <div className="flex-1 min-h-0 overflow-y-auto pb-1">
              {grouped.length === 0 ? (
                <div className="px-4 py-8 text-center text-xs text-slate-500 font-mono">
                  <T id="persona.noMatch" values={{ query }} />
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
                      };
                    }
                    if (groupBy === 'role') {
                      const rk = (head?.role_id || head?.role_name || gKey) as DisplayRoleName;
                      return {
                        icon: roleGlyph(rk as any) || 'badge',
                        label: roleLabel(rk as any) || gKey,
                        code: (head?.role_id || gKey).toString().toUpperCase().slice(0, 6),
                      };
                    }
                    const dKey = gKey !== '__other__' ? gKey : (head?.department || 'department');
                    const m = deptMetaFromKey(dKey);
                    return { icon: m.icon, label: m.label, code: m.code };
                  })();
                  return (
                    <div key={gKey}>
                      {gi > 0 && <div className="my-1.5 mx-2 border-t border-slate-800/80" />}
                      <div className="mx-1 mb-1 px-2 py-1 flex items-center gap-2 rounded-md bg-slate-900/50 border border-slate-800">
                        <span className="text-xs">{gMeta.icon}</span>
                        {groupBy === 'level' ? (
                          <span className="text-xs uppercase tracking-widest font-mono font-bold text-slate-300">
                            <T id={`persona.level.${Number(gKey.replace(/^P/, ''))}`} />
                          </span>
                        ) : (
                          <span className="text-xs uppercase tracking-widest font-mono font-bold text-slate-300">                           <T id={gMeta.label} /></span>
                        )}
                        <span className="ml-auto text-[8px] font-mono px-1.5 py-0.5 rounded border border-slate-700 bg-slate-950/60 text-slate-300">
                          {gMeta.code} • {arr.length}
                        </span>
                      </div>
                      <div className="space-y-0.5">
                        {arr.map((u: any) => {
                          const idx = flat.findIndex((f) => f.id === u.id);
                          const selected = currentUser?.id === u.id;
                          const focused = idx === highlight;
                          const roleKey = u.role_id || u.role_name;
                          return (
                            <button
                              key={u.id}
                              role="menuitem"
                              type="button"
                              onClick={() => selectUser(u)}
                              onMouseEnter={() => setHighlight(idx)}
                              className={[
                                'w-full flex items-center gap-2.5 px-2 py-2 rounded-xl text-left transition-all',
                                focused ? 'bg-slate-900' : 'bg-transparent',
                                selected ? 'border border-indigo-500/40' : 'border border-transparent hover:border-slate-800',
                              ].join(' ')}
                            >
                              <UserAvatar
                                fullname={u.fullname}
                                role={roleKey}
                                level={staffLevelOf(u)}
                                size="xs"
                              />
                              <div className="flex-1 min-w-0">
                                <div className="flex items-center gap-1.5">
                                  <span className="text-xs font-bold truncate text-white">{u.fullname}</span>
                                  {selected && <span className="text-emerald-400 text-xs font-bold">✔</span>}
                                </div>
                                <div className="flex items-center gap-1 mt-1 flex-wrap">
                                  <span
                                    className={[
                                      'px-1.5 py-0.5 rounded text-[8px] font-mono font-bold uppercase border',
                                      roleBadge(roleKey),
                                    ].join(' ')}
                                  >
                                    {positionLabel(roleKey, u.department ?? null)}
                                  </span>
                                  <span className="px-1.5 py-0.5 rounded text-[8px] font-mono font-bold uppercase border bg-slate-500/10 text-slate-300 border-slate-600/50">
                                    P{staffLevelOf(u)}
                                  </span>
                                  <span className="text-xs text-slate-600 font-mono">{u.employee_code}</span>
                                </div>
                              </div>
                              <span className="text-base leading-none opacity-70 shrink-0">{roleGlyph(roleKey)}</span>
                            </button>
                          );
                        })}
                      </div>
                    </div>
                  );
                })
              )}
            </div>

            <div className="px-3 py-2 border-t border-slate-800/80 flex items-center justify-between gap-2 text-xs text-slate-500 font-mono">
              <span className="shrink-0"><T id="persona.filterHint" /></span>
              <span className="flex items-center gap-1.5">
                <span className={['px-1.5 py-0.5 rounded border', roleBadge(curRole)].join(' ')}>
                  ● {positionLabel(curRole, currentUser?.department ?? null)}
                </span>
                <button
                  type="button"
                  onClick={signOut}
                  className="px-1.5 py-0.5 rounded border border-rose-500/40 bg-rose-500/10 text-rose-200 hover:bg-rose-500/20 transition-colors font-bold uppercase tracking-wider"
                >
                  <T id="chrome.signOut" />
                </button>
              </span>
            </div>
          </div>
        </>
      )}
    </div>
  );
};

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
