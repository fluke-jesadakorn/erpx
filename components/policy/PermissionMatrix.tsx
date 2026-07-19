'use client';

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { useRouter } from 'next/navigation';
import {
  Building2,
  Check,
  ChevronDown,
  CircleAlert,
  Info,
  Key,
  LoaderCircle,
  Search,
  Shield,
  User,
  UserCheck,
  Users,
  type LucideIcon,
} from 'lucide-react';

interface AdminColumn {
  perm: string;
  domain: string;
  subject: string;
  verb: string;
  description: string | null;
}

interface AdminTarget {
  id: string;
  kind: 'department' | 'role';
  label: string;
  significance: boolean;
  member_count: number;
  is_seed_persona: boolean;
  is_system: boolean;
  role_kind?: 'hierarchy' | 'system' | null;
  rank?: number | null;
}

interface Props {
  columns: AdminColumn[];
  targets: AdminTarget[];
  initialCells: Record<string, string[]>;
  canEdit: boolean;
  actorName: string;
}

interface UserRow {
  id: number;
  fullname: string;
  employee_code: string;
  department: string | null;
  role_id: string | null;
  perm_role_ids?: string[];
}

interface UserDetail {
  user: {
    id: number;
    fullname: string;
    employee_code: string;
    department: string | null;
    role_id: string | null;
    perm_role_ids: string[];
    perm_role_names: string[];
  };
  grants: string[];
  active_perm_ids: string[];
}

const DOMAIN_META: Record<string, { code: string; tone: 'positive' | 'info' | 'accent' | 'caution' | 'critical' }> = {
  finance:        { code: 'FIN', tone: 'positive' },
  stage:          { code: 'STG', tone: 'info' },
  tile:           { code: 'TIL', tone: 'neutral' as 'info' },
  user:           { code: 'USR', tone: 'info' },
  org:            { code: 'ORG', tone: 'positive' },
  rbac:           { code: 'RBC', tone: 'critical' },
  ai:             { code: 'AI',  tone: 'accent' },
  hook:           { code: 'HK',  tone: 'caution' },
  hr:             { code: 'HR',  tone: 'accent' },
  law:            { code: 'LW',  tone: 'accent' },
  access_request: { code: 'ACR', tone: 'caution' },
  admin:          { code: 'ADM', tone: 'critical' },
  system:         { code: 'SYS', tone: 'neutral' as 'info' },
};

function metaFor(domain: string) {
  return DOMAIN_META[domain] ?? { code: domain.slice(0, 3).toUpperCase(), tone: 'info' as const };
}

const TONE = {
  positive: { text: 'text-positive',     soft: 'bg-positive-soft/25', active: 'bg-positive-soft/55', border: 'border-positive/55', dot: 'bg-positive' },
  info:     { text: 'text-info',         soft: 'bg-info-soft/25',     active: 'bg-info-soft/55',     border: 'border-info/55',     dot: 'bg-info' },
  accent:   { text: 'text-accent-strong',soft: 'bg-accent-soft/25',   active: 'bg-accent-soft/55',   border: 'border-accent/55',   dot: 'bg-accent' },
  caution:  { text: 'text-caution',      soft: 'bg-caution-soft/25',  active: 'bg-caution-soft/55',  border: 'border-caution/55',  dot: 'bg-caution' },
  critical: { text: 'text-critical',     soft: 'bg-critical-soft/25', active: 'bg-critical-soft/55', border: 'border-critical/55', dot: 'bg-critical' },
  neutral:  { text: 'text-mute',         soft: 'bg-paper-3/40',       active: 'bg-paper-3/60',       border: 'border-rule/60',     dot: 'bg-mute' },
} as const;

type ToneKey = keyof typeof TONE;

const BADGE_TONE = {
  positive: 'bg-positive-soft text-positive border-positive/40',
  caution:  'bg-caution-soft text-caution border-caution/40',
  critical: 'bg-critical-soft text-critical border-critical/40',
  info:     'bg-info-soft text-info border-info/40',
  accent:   'bg-accent-soft text-accent-strong border-accent/40',
  neutral:  'bg-paper-3 text-mute border-rule',
} as const;

function SidebarBadge({ count, tone = 'neutral' }: { count?: number | string; tone?: keyof typeof BADGE_TONE }) {
  if (count == null) return null;
  return (
    <span className={[
      'ml-auto inline-flex h-5 min-w-5 items-center justify-center rounded-md border px-1.5 text-[11px] font-semibold tabular-nums',
      BADGE_TONE[tone],
    ].join(' ')}>
      {typeof count === 'number' && count > 99 ? '99+' : count}
    </span>
  );
}

function PanelShell({ children, className }: { children: React.ReactNode; className?: string }) {
  return (
    <div className={[
      'flex flex-col overflow-hidden rounded-md border-rule bg-paper-2/60',
      className || '',
    ].join(' ')}>
      {children}
    </div>
  );
}

function PanelHeader({
  icon,
  label,
  count,
  tone,
  right,
  collapsed,
  onToggle,
}: {
  icon: LucideIcon;
  label: string;
  count?: number;
  tone: ToneKey;
  right?: React.ReactNode;
  collapsed?: boolean;
  onToggle?: () => void;
}) {
  const t = TONE[tone];
  const IconCmp = icon;
  return (
    <div className={['flex items-center gap-2.5 px-1 py-3'].join(' ')}>
      <span className={['flex h-6 w-6 shrink-0 items-center justify-center', t.text].join(' ')} aria-hidden>
        <IconCmp size={15} />
      </span>
      <span className="text-[12px] font-semibold uppercase tracking-[0.1em] text-mute">
        {label}
      </span>
      {typeof count === 'number' ? <SidebarBadge count={count} tone="neutral" /> : null}
      <div className="ml-auto flex items-center gap-2">{right}</div>
      {onToggle ? (
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={!collapsed}
          aria-label={collapsed ? `Expand ${label}` : `Collapse ${label}`}
          className={['flex h-6 w-6 items-center justify-center rounded-md border border-rule/40 hover:bg-paper-3/40 transition-colors', t.text].join(' ')}
        >
          <ChevronDown size={13} className={['transition-transform duration-200', collapsed ? '-rotate-90' : 'rotate-0'].join(' ')} />
        </button>
      ) : null}
    </div>
  );
}

function SearchInput({
  value,
  onChange,
  placeholder,
  disabled,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder: string;
  disabled?: boolean;
}) {
  return (
    <div className={['relative', disabled ? 'opacity-50' : ''].join(' ')}>
      <span aria-hidden className="absolute left-3 top-1/2 -translate-y-1/2 text-mute">
        <Search size={14} />
      </span>
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        disabled={disabled}
        className="w-full pl-8 pr-3 h-9 border-0 border-b border-rule/40 bg-transparent text-[13.5px] text-ink rounded-none placeholder:text-mute/70 focus:outline-none focus:border-accent/60 transition-colors"
      />
    </div>
  );
}

function NavRow({
  active,
  locked,
  tone,
  dot,
  left,
  center,
  right,
  rightAction,
  onClick,
}: {
  active?: boolean;
  locked?: boolean;
  tone: ToneKey;
  dot?: boolean;
  left?: React.ReactNode;
  center?: React.ReactNode;
  right?: React.ReactNode;
  rightAction?: React.ReactNode;
  onClick?: () => void;
}) {
  const t = TONE[tone];
  const base = 'group relative flex items-stretch gap-3 px-1 py-2 transition-colors duration-150 w-full';
  const state = locked
    ? 'text-mute/70 opacity-60 cursor-not-allowed'
    : active
      ? `${t.active} text-ink`
      : 'text-ink-2 hover:bg-paper-3/40 hover:text-ink cursor-pointer';

  const marker = dot ? (
    <span
      aria-hidden
      className={[
        'absolute left-0 top-0 bottom-0 w-1 transition-all duration-200',
        active
          ? `${t.dot} shadow-[0_0_10px_-1px_currentColor]`
          : 'bg-transparent group-hover:bg-rule/40',
      ].join(' ')}
    />
  ) : null;

  const main = (
    <>
      {left ? (
        <span className={['flex h-5 w-5 shrink-0 items-center justify-center transition-colors', active ? t.text : 'text-mute group-hover:text-ink-2'].join(' ')} aria-hidden>
          {left}
        </span>
      ) : null}
      <span className="flex min-w-0 flex-1 items-center text-left">{center}</span>
    </>
  );

  const rightSlot = right ? <span className="flex shrink-0 items-center gap-1.5">{right}</span> : null;
  const rightActionSlot = rightAction ? <span className="flex shrink-0 items-center gap-1.5">{rightAction}</span> : null;

  if (locked || !onClick) {
    return <div className={[base, state].join(' ')}>{marker}{main}{rightSlot}{rightActionSlot}</div>;
  }

  if (!rightAction) {
    return (
      <button type="button" onClick={onClick} aria-current={active ? 'true' : undefined} className={[base, state].join(' ')}>
        {marker}{main}{rightSlot}
      </button>
    );
  }

  return (
    <div className={[base, state].join(' ')}>
      {marker}
      <button
        type="button"
        onClick={onClick}
        aria-current={active ? 'true' : undefined}
        className="flex min-w-0 flex-1 items-stretch gap-3 border-0 bg-transparent p-0 text-left text-inherit"
      >
        {main}{rightSlot}
      </button>
      {rightActionSlot}
    </div>
  );
}

function Pill({ children, tone = 'neutral', size = 'sm' }: { children: React.ReactNode; tone?: ToneKey; size?: 'xs' | 'sm' }) {
  const sz = size === 'xs' ? 'text-[10px] px-1.5 py-0.5' : 'text-[11px] px-2 py-0.5';
  return (
    <span className={['inline-flex items-center gap-1 border font-bold uppercase tracking-[0.1em] rounded-md whitespace-nowrap', sz, BADGE_TONE[tone]].join(' ')}>
      {children}
    </span>
  );
}

function CheckPill({ checked, onChange, disabled, tone }: { checked: boolean; onChange: () => void; disabled?: boolean; tone: ToneKey }) {
  const t = TONE[tone];
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      aria-label={checked ? 'Assigned' : 'Not assigned'}
      disabled={disabled}
      onClick={onChange}
      className={[
        'inline-flex h-6 w-6 items-center justify-center rounded-md border-2 transition-all duration-200',
        checked
          ? `${t.active} ${t.border} ${t.text}`
          : 'bg-paper border-rule text-transparent hover:border-accent/60',
        disabled ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer',
      ].join(' ')}
    >
      {checked ? <Check size={14} /> : null}
    </button>
  );
}

const COLLAPSE_KEY = 'folio.policy.collapsed';

function readCollapsed(): Record<string, boolean> {
  if (typeof window === 'undefined') return {};
  try {
    const raw = window.sessionStorage.getItem(COLLAPSE_KEY);
    return raw ? (JSON.parse(raw) as Record<string, boolean>) : {};
  } catch {
    return {};
  }
}

function writeCollapsed(map: Record<string, boolean>) {
  if (typeof window === 'undefined') return;
  try {
    window.sessionStorage.setItem(COLLAPSE_KEY, JSON.stringify(map));
  } catch {
    /* ignore */
  }
}

export function PermissionMatrix({ columns, targets, initialCells, canEdit, actorName }: Props) {
  const router = useRouter();

  const [users, setUsers] = useState<UserRow[] | null>(null);
  const [activeUserId, setActiveUserId] = useState<number | null>(null);
  const [userDetail, setUserDetail] = useState<UserDetail | null>(null);
  const [activeTargetId, setActiveTargetId] = useState<string | null>(null);
  const [pendingTargetPerms, setPendingTargetPerms] = useState<Set<string> | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [userQuery, setUserQuery] = useState('');
  const [permQuery, setPermQuery] = useState('');
  const [collapsedMap, setCollapsedMap] = useState<Record<string, boolean>>({});
  const [hydrated, setHydrated] = useState(false);

  useEffect(() => {
    setCollapsedMap(readCollapsed());
    setHydrated(true);
  }, []);

  const toggleSection = useCallback((key: string) => {
    setCollapsedMap((prev) => {
      const next = { ...prev, [key]: !prev[key] };
      writeCollapsed(next);
      return next;
    });
  }, []);

  const isCollapsed = useCallback(
    (key: string) => (hydrated ? Boolean(collapsedMap[key]) : false),
    [hydrated, collapsedMap],
  );

  const refreshUsers = useCallback(async () => {
    const r = await fetch('/api/perm/users');
    const j = await r.json();
    setUsers(j.users ?? []);
  }, []);

  useEffect(() => {
    refreshUsers().catch(() => setUsers([]));
  }, [refreshUsers]);

  useEffect(() => {
    if (activeUserId == null) {
      setUserDetail(null);
      return;
    }
    fetch(`/api/perm/users/${activeUserId}/grants`)
      .then((r) => r.json())
      .then(setUserDetail)
      .catch(() => setUserDetail(null));
  }, [activeUserId]);

  const departmentTargets = useMemo(() => targets.filter((tg) => tg.kind === 'department'), [targets]);
  const roleTargets = useMemo(() => targets.filter((tg) => tg.kind === 'role'), [targets]);

  const activeTarget = useMemo(
    () => targets.find((tg) => tg.id === activeTargetId) ?? null,
    [targets, activeTargetId],
  );

  const baseTargetPerms = useMemo(() => {
    if (!activeTarget) return new Set<string>();
    return new Set(initialCells[activeTarget.id] ?? []);
  }, [activeTarget, initialCells]);
  const targetPerms = pendingTargetPerms ?? baseTargetPerms;

  const pickUser = (u: UserRow) => {
    if (activeUserId === u.id) return;
    setActiveUserId(u.id);
    setActiveTargetId(null);
    setPendingTargetPerms(null);
  };

  const pickTarget = (tg: AdminTarget) => {
    if (!userDetail) return;
    if (activeTargetId === tg.id) return;
    setActiveTargetId(tg.id);
    setPendingTargetPerms(null);
  };

  const toggleTargetPerm = async (permId: string) => {
    if (!activeTarget) return;
    const next = new Set(targetPerms);
    if (next.has(permId)) next.delete(permId);
    else next.add(permId);
    setPendingTargetPerms(next);
    setSaving(true);
    setError(null);
    try {
      const r = await fetch('/api/policy/grants', {
        method: 'PUT',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          entries: [{ target_kind: activeTarget.kind, target_id: activeTarget.id, allow: Array.from(next) }],
        }),
      });
      if (!r.ok) {
        const j = await r.json().catch(() => ({}));
        throw new Error(j.error || `HTTP ${r.status}`);
      }
      router.refresh();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to update permissions');
    } finally {
      setSaving(false);
      setPendingTargetPerms(null);
    }
  };

  const filteredUsers = useMemo(() => {
    if (!users) return [];
    const q = userQuery.trim().toLowerCase();
    if (!q) return users;
    return users.filter(
      (u) =>
        u.fullname.toLowerCase().includes(q) ||
        u.employee_code.toLowerCase().includes(q) ||
        (u.department ?? '').toLowerCase().includes(q) ||
        (u.role_id ?? '').toLowerCase().includes(q),
    );
  }, [users, userQuery]);

  const filteredColumns = useMemo(() => {
    const q = permQuery.trim().toLowerCase();
    if (!q) return columns;
    return columns.filter(
      (c) =>
        c.subject.toLowerCase().includes(q) ||
        c.verb.toLowerCase().includes(q) ||
        c.perm.toLowerCase().includes(q) ||
        c.domain.toLowerCase().includes(q),
    );
  }, [columns, permQuery]);

  const selectedUser = useMemo(() => users?.find((u) => u.id === activeUserId) ?? null, [users, activeUserId]);
  const userActivePerms = useMemo(() => new Set(userDetail?.active_perm_ids ?? []), [userDetail]);

  const usersCollapsed = isCollapsed('users');
  const targetsCollapsed = isCollapsed('targets');
  const permsCollapsed = isCollapsed('perms');

  return (
    <div className="min-h-screen text-ink">
      <main className="max-w-[1700px] mx-auto px-4 py-3 md:px-6 md:py-4 space-y-3">
        {error ? (
          <div className="flex items-center gap-2 px-3 py-2 border border-critical/45 bg-critical-soft/40 text-critical text-[12px] font-mono rounded-lg shadow-[var(--shadow-popover)]">
            <CircleAlert size={12} />
            <span>{error}</span>
          </div>
        ) : null}

        {saving ? (
          <div className="flex items-center gap-2 px-3 py-2 border border-info/45 bg-info-soft/30 text-info text-[12px] font-mono rounded-lg shadow-[var(--shadow-popover)]">
            <LoaderCircle size={12} className="animate-spin" />
            <span>Saving…</span>
          </div>
        ) : null}

        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
          <PanelShell>
            <PanelHeader
              icon={Users}
              label="Users"
              tone="positive"
              count={users?.length ?? 0}
              collapsed={usersCollapsed}
              onToggle={() => toggleSection('users')}
              right={
                selectedUser ? (
                  <Pill tone="positive" size="xs">
                    {selectedUser.fullname.split(/\s+/)[0]}
                  </Pill>
                ) : (
                  <span className="text-[11px] text-mute/70 font-medium uppercase tracking-[0.1em]">pick one</span>
                )
              }
            />
            <div
              className={['grid transition-[grid-template-rows] duration-200  ease-out', usersCollapsed ? 'grid-rows-[0fr]' : 'grid-rows-[1fr]'].join(' ')}
            >
              <div className="overflow-hidden">
                <div>
                  <div className="px-1 pt-2 pb-3">
                    <SearchInput value={userQuery} onChange={setUserQuery} placeholder="Search user" />
                  </div>
                  <div className="overflow-auto max-h-[68vh] divide-y divide-rule/30">
                    {filteredUsers.length === 0 ? (
                      <div className="px-1 py-6 text-center text-[12px] text-mute">
                        {users === null ? 'Loading users…' : 'No users match the filter.'}
                      </div>
                    ) : (
                      filteredUsers.map((u) => {
                        const isActive = u.id === activeUserId;
                        return (
                          <NavRow
                            key={u.id}
                            tone="positive"
                            active={isActive}
                            dot
                            left={<User size={13} />}
                            onClick={() => pickUser(u)}
                            center={
                              <span className="flex min-w-0 flex-col leading-snug">
                                <span className={['truncate text-[15px] transition-colors', isActive ? 'font-bold' : 'font-semibold'].join(' ')}>{u.fullname}</span>
                                <span className="truncate text-[12px] text-mute/80 font-mono">{u.employee_code}{u.role_id ? ` · ${u.role_id}` : ''}</span>
                              </span>
                            }
                            right={u.department ? <Pill tone="info" size="xs">{u.department}</Pill> : null}
                          />
                        );
                      })
                    )}
                  </div>
                </div>
              </div>
            </div>
          </PanelShell>

          <PanelShell>
            <PanelHeader
              icon={Building2}
              label="Targets · roles · departments"
              tone="info"
              count={targets.length}
              collapsed={targetsCollapsed}
              onToggle={() => toggleSection('targets')}
              right={
                activeTarget ? (
                  <Pill tone="info" size="xs">
                    {activeTarget.kind === 'department' ? 'DEPT' : 'ROLE'} {activeTarget.label}
                  </Pill>
                ) : (
                  <span className="text-[11px] text-mute/70 font-medium uppercase tracking-[0.1em]">{userDetail ? 'pick a target' : 'pick a user first'}</span>
                )
              }
            />
            <div
              className={['grid transition-[grid-template-rows] duration-200 ease-out', targetsCollapsed ? 'grid-rows-[0fr]' : 'grid-rows-[1fr]'].join(' ')}
            >
              <div className="overflow-hidden">
                <div>
                  {!userDetail ? (
                    <div className="px-1 py-8 text-center text-[12px] text-mute">
                      Pick a user on the left to manage their roles &amp; departments.
                    </div>
                  ) : (
                    <div className="overflow-auto max-h-[68vh] divide-y divide-rule/40">
                      <div>
                        <SubHeader label="Departments" tone="info" count={departmentTargets.length} />
                        <div>
                          {departmentTargets.map((d) => {
                            const isActive = activeTargetId === d.id;
                            return (
                              <NavRow
                                key={d.id}
                                tone="info"
                                active={isActive}
                                dot
                                left={<Building2 size={13} />}
                                onClick={() => pickTarget(d)}
                                center={
                                  <span className="flex min-w-0 flex-col leading-snug">
                                    <span className={['truncate text-[14px] transition-colors', isActive ? 'font-bold' : 'font-semibold'].join(' ')}>{d.label}</span>
                                    <span className="truncate text-[12px] text-mute/80 font-mono">{d.id}</span>
                                  </span>
                                }
                                right={<span className="text-[12px] text-mute tabular-nums">{d.member_count} mem</span>}
                              />
                            );
                          })}
                        </div>

                        <SubHeader label="Specific roles" tone="accent" count={roleTargets.length} />
                        <div>
                          {roleTargets.map((r) => {
                            const isActive = activeTargetId === r.id;
                            return (
                              <NavRow
                                key={r.id}
                                tone="accent"
                                active={isActive}
                                dot
                                left={<Shield size={13} />}
                                onClick={() => pickTarget(r)}
                                center={
                                  <span className="flex min-w-0 flex-col leading-snug">
                                    <span className={['truncate text-[14px] transition-colors', isActive ? 'font-bold' : 'font-semibold'].join(' ')}>{r.label}</span>
                                    <span className="truncate text-[12px] text-mute/80 font-mono">{r.id}</span>
                                  </span>
                                }
                                right={<span className="text-[12px] text-mute tabular-nums">{r.member_count} mem</span>}
                              />
                            );
                          })}
                        </div>
                      </div>
                    </div>
                  )}
                </div>
              </div>
            </div>
          </PanelShell>

          <PanelShell>
            <PanelHeader
              icon={Key}
              label="Permissions"
              tone="accent"
              count={activeTarget ? filteredColumns.length : columns.length}
              collapsed={permsCollapsed}
              onToggle={() => toggleSection('perms')}
              right={
                activeTarget ? (
                  <span className="flex items-center gap-1.5">
                    <span className="inline-flex items-center gap-1 px-1.5 py-0.5 border border-positive/40 bg-positive-soft/30 text-positive text-[11px] font-mono font-bold rounded-md">
                      <Check size={10} />
                      {targetPerms.size}
                    </span>
                    <span className="text-[11px] text-mute/70 uppercase tracking-[0.1em]">assigned</span>
                  </span>
                ) : (
                  <span className="text-[11px] text-mute/70 font-medium uppercase tracking-[0.1em]">pick a target</span>
                )
              }
            />
            <div
              className={['grid transition-[grid-template-rows] duration-200 ease-out', permsCollapsed ? 'grid-rows-[0fr]' : 'grid-rows-[1fr]'].join(' ')}
            >
              <div className="overflow-hidden">
                <div>
                  <div className="px-1 pt-2 pb-3">
                    <SearchInput
                      value={permQuery}
                      onChange={setPermQuery}
                      placeholder="Search permission"
                      disabled={!activeTarget}
                    />
                  </div>
                  <div className="overflow-auto max-h-[68vh] divide-y divide-rule/30">
                    {!activeTarget ? (
                      <div className="px-3 py-8 text-center text-[12px] text-mute">
                        Pick a target on the middle column to toggle its permissions.
                      </div>
                    ) : filteredColumns.length === 0 ? (
                      <div className="px-3 py-8 text-center text-[12px] text-mute">
                        No permissions match the filter.
                      </div>
                    ) : (
                      filteredColumns.map((c) => {
                        const checked = targetPerms.has(c.perm);
                        const userHas = userActivePerms.has(c.perm);
                        const m = metaFor(c.domain);
                        const isActive = checked;
                        return (
                          <NavRow
                            key={c.perm}
                            tone="accent"
                            active={isActive}
                            dot
                            onClick={() => canEdit && toggleTargetPerm(c.perm)}
                            locked={!canEdit}
                            left={<Key size={13} />}
                            center={
                              <span className="flex min-w-0 flex-col leading-snug">
                                <span className={['truncate text-[14px] transition-colors', isActive ? 'font-bold' : 'font-semibold'].join(' ')}>
                                  <span className="text-mute/80 font-mono">{m.code}</span>
                                  <span className="px-1 text-mute/40">·</span>
                                  {c.subject}
                                  <span className="ml-1.5 text-mute">›</span>
                                  <span className={['ml-1.5', isActive ? 'font-extrabold' : 'font-bold'].join(' ')}>{c.verb}</span>
                                </span>
                                <span className="truncate text-[12px] text-mute/80 font-mono" title={c.perm}>
                                  {c.perm}
                                </span>
                              </span>
                            }
                            rightAction={
                              <span className="flex items-center gap-1.5">
                                {userHas ? (
                                  <Pill tone="positive" size="xs">
                                    <UserCheck size={10} />
                                    user
                                  </Pill>
                                ) : null}
                                <CheckPill checked={checked} onChange={() => toggleTargetPerm(c.perm)} disabled={!canEdit} tone={m.tone} />
                              </span>
                            }
                          />
                        );
                      })
                    )}
                  </div>
                </div>
              </div>
            </div>
          </PanelShell>
        </div>

        <footer className="flex items-center justify-between gap-2 px-3 py-2 text-[12px] text-mute/80 font-mono">
          <span className="flex items-center gap-2">
            <Info size={12} className="text-mute/70" />
            {users?.length ?? 0} users · {departmentTargets.length} departments · {roleTargets.length} roles · {columns.length} permissions
          </span>
          <span className="flex items-center gap-2">
            <span className={['h-2 w-2 rounded-full', canEdit ? 'bg-positive' : 'bg-mute'].join(' ')} aria-hidden />
            {canEdit ? 'edit mode' : 'view only'} · {actorName || 'actor'}
          </span>
        </footer>
      </main>
    </div>
  );
}

function SubHeader({ label, tone, count }: { label: string; tone: ToneKey; count: number }) {
  const t = TONE[tone];
  return (
    <div className={['flex items-center gap-2.5 px-1 py-2'].join(' ')}>
      <span aria-hidden className={['h-1.5 w-5 rounded-full', t.dot].join(' ')} />
      <span className="text-[11px] font-semibold uppercase tracking-[0.1em] text-mute">{label}</span>
      <SidebarBadge count={count} tone={tone} />
    </div>
  );
}
