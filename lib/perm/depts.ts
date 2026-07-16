export const DEPT_LABEL: Record<string, string> = {
  development: 'departments.development',
  executive: 'departments.executive',
  finance: 'departments.finance',
  hr: 'departments.hr',
  it: 'departments.it',
  marketing: 'departments.marketing',
};

export const DEPT_ICON: Record<string, string> = {
  development: '🛠️',
  executive: '👑',
  finance: '💰',
  hr: '🤝',
  it: '💻',
  marketing: '📣',
};

const DEPT_CODE: Record<string, string> = {
  development: 'ENG',
  executive: 'EXE',
  finance: 'FIN',
  hr: 'HR',
  it: 'IT',
  marketing: 'MKT',
};

export function deptLabel(key: string | null | undefined): string {
  if (!key) return 'departments.department';
  return DEPT_LABEL[key] ?? `departments.${key}`;
}

export function deptIcon(key: string | null | undefined): string {
  if (!key) return '🏢';
  return DEPT_ICON[key] || '🏢';
}

export function deptCode(key: string | null | undefined): string {
  if (!key) return 'DEP';
  return DEPT_CODE[key] ?? (key.replace(/[^a-z0-9]/gi, '').slice(0, 3).toUpperCase() || 'DEP');
}
