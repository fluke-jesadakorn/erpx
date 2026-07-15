// lib/perm/depts.ts — canonical department id → display label.
//
// The DB column perm.user_permissions holds `user:dept:<id>::allow` grants.
// The <id> is the canonical department key (development, executive, finance,
// hr, it, marketing). This module maps the key to a readable label and icon.

export const DEPT_LABEL: Record<string, string> = {
  development: 'Engineering',
  executive:   'Executive',
  finance:     'Finance',
  hr:          'HR',
  it:          'IT',
  marketing:   'Marketing',
};

export const DEPT_ICON: Record<string, string> = {
  development: '🛠️',
  executive:   '👑',
  finance:     '💰',
  hr:          '🤝',
  it:          '💻',
  marketing:   '📣',
};

export function deptLabel(key: string | null | undefined): string {
  if (!key) return 'Department';
  return DEPT_LABEL[key] || key.charAt(0).toUpperCase() + key.slice(1);
}

export function deptIcon(key: string | null | undefined): string {
  if (!key) return '🏢';
  return DEPT_ICON[key] || '🏢';
}

export function deptCode(key: string | null | undefined): string {
  if (!key) return 'DEP';
  const lbl = deptLabel(key);
  return lbl.split(/\s+/).map((w) => w[0] || '').join('').slice(0, 3).toUpperCase() || 'DEP';
}
