import React from 'react';
import { staffLevelAccent, staffLevelBadge, staffLevelLabel, type StaffLevel } from '@/org/display';
import {
  ROLE_GLYPH,
  ROLE_ACCENT,
  ROLE_LABEL,
  ROLE_BADGE,
  type DisplayRoleName,
} from '@/org/display';

export { staffLevelAccent, staffLevelBadge, staffLevelLabel, type StaffLevel };

const STAFF_LEVEL_GLYPH: Record<number, string> = {
  1: '👑',
  2: '💼',
  3: '🛡️',
  4: '👥',
  5: '👤',
};

export function staffLevelGlyph(level: number | null | undefined): string {
  if (level === 1 || level === 2 || level === 3 || level === 4 || level === 5) {
    return STAFF_LEVEL_GLYPH[level];
  }
  return '·';
}

export function roleNameOf(role?: string | null): string {
  if (!role) return '';
  const i = role.indexOf('::');
  return i >= 0 ? role.slice(0, i) : role;
}

export function roleAccent(role?: string): string {
  return ROLE_ACCENT[roleNameOf(role) as DisplayRoleName] || 'from-slate-500 to-slate-700';
}

export function roleGlyph(role?: string): string {
  return ROLE_GLYPH[roleNameOf(role) as DisplayRoleName] || '👤';
}

export function roleLabel(role?: string): string {
  if (!role) return 'Unknown';
  const key = roleNameOf(role);
  return ROLE_LABEL[key as DisplayRoleName] || key || 'Unknown';
}

export function roleBadge(role?: string): string {
  return ROLE_BADGE[roleNameOf(role) as DisplayRoleName] || 'bg-slate-500/15 text-slate-200 border-slate-500/40';
}

const LEVEL_ACCENT: Record<string, string> = {
  root:   'from-slate-500 to-slate-700',
  junior: 'from-teal-500 to-cyan-700',
  mid:    'from-sky-500 to-blue-700',
  senior: 'from-amber-500 to-orange-700',
  elite:  'from-rose-500 to-pink-700',
};

export function levelAccent(level?: number | null): string | null {
  if (typeof level !== 'number') return null;
  if (level === 1 || level === 2 || level === 3 || level === 4 || level === 5) {
    return staffLevelAccent(level);
  }
  return 'from-slate-500 to-slate-700';
}

function initialsOf(name?: string): string {
  if (!name) return '?';
  const parts = name.trim().split(/\s+/);
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

interface UserAvatarProps {
  fullname?: string;
  role?: string;
  level?: number | null;
  size?: 'xs' | 'sm' | 'md' | 'lg';
  ring?: boolean;
  className?: string;
}

const SIZE: Record<string, string> = {
  xs: 'w-7 h-7 text-xs',
  sm: 'w-8 h-8 text-xs',
  md: 'w-10 h-10 text-sm',
  lg: 'w-12 h-12 text-base',
};

export const UserAvatar: React.FC<UserAvatarProps> = ({
  fullname,
  role,
  level,
  size = 'md',
  ring = false,
  className = '',
}) => {
  const accent = levelAccent(level) ?? roleAccent(role);
  const initials = initialsOf(fullname);
  return (
    <div
      className={[
        'inline-flex items-center justify-center rounded-full bg-gradient-to-br text-white font-black font-mono shrink-0 select-none',
        accent,
        SIZE[size],
        ring ? 'ring-2 ring-offset-2 ring-offset-slate-950 ring-indigo-500/40' : '',
        className,
      ].join(' ')}
      aria-label={fullname || 'user avatar'}
      title={fullname}
    >
      {initials}
    </div>
  );
};
