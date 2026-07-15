// Single source of truth for role display metadata. Previously duplicated
// across 6+ files: UserAvatar.tsx, SignInPanel.tsx, NoPermissionView.tsx,
// PersonaMenu.tsx, lib/orgScope.ts, lib/autoWire.server.ts, OrgChartView.tsx,
// UserDirectoryView.tsx.
//
// Keep this file as the only place where role-name → UI metadata lives.

export type StaffLevel = number;
export type RoleName = string;
export type TabName = string;
export type ActionName = string;

export type DisplayRoleName =
  | 'staff'
  | 'accountant'
  | 'account_officer'
  | 'account_supervisor'
  | 'accounting_manager'
  | 'supervisor'
  | 'head_of_department'
  | 'admin'
  | 'cfo'
  | 'ceo'
  | 'it'
  | 'hr'
  | 'hr_manager'
  | 'manager'
  | 'finance'
  | 'sales_rep'
  | 'sales_supervisor'
  | 'officer';

export const ROLE_GLYPH: Record<DisplayRoleName, string> = {
  staff:               '🧑‍💼',
  accountant:          '🧮',
  account_officer:     '📋',
  account_supervisor:  '📊',
  accounting_manager:  '🧾',
  supervisor:          '👥',
  head_of_department:  '🛡️',
  admin:               '🛠️',
  cfo:                 '💼',
  ceo:                 '👑',
  it:                  '💻',
  hr:                  '🤝',
  hr_manager:          '🪪',
  manager:             '📈',
  finance:             '💰',
  officer:             '🪖',
  sales_rep:           '🛒',
  sales_supervisor:    '🛍️',
};

export const ROLE_ACCENT: Record<DisplayRoleName, string> = {
  staff:               'from-emerald-500 to-teal-700',
  accountant:          'from-sky-500 to-blue-700',
  account_officer:     'from-cyan-500 to-indigo-700',
  account_supervisor:  'from-indigo-500 to-blue-700',
  accounting_manager:  'from-amber-500 to-orange-700',
  supervisor:          'from-cyan-500 to-sky-700',
  head_of_department:  'from-amber-500 to-yellow-700',
  admin:               'from-purple-500 to-fuchsia-700',
  cfo:                 'from-purple-500 to-fuchsia-700',
  ceo:                 'from-rose-500 to-pink-700',
  it:                  'from-slate-500 to-slate-700',
  hr:                  'from-pink-500 to-rose-700',
  hr_manager:          'from-fuchsia-500 to-pink-700',
  manager:             'from-cyan-500 to-blue-700',
  finance:             'from-emerald-500 to-green-700',
  officer:             'from-slate-500 to-slate-700',
  sales_rep:           'from-orange-500 to-amber-700',
  sales_supervisor:    'from-amber-500 to-yellow-700',
};

export const ROLE_LABEL: Record<DisplayRoleName, string> = {
  staff:               'Staff Requester',
  accountant:          'Auditor / Accountant',
  account_officer:     'Account Officer',
  account_supervisor:  'Account Supervisor',
  accounting_manager:  'Accounting Manager',
  supervisor:          'Supervisor',
  head_of_department:  'Head of Department',
  manager:             'Manager',
  admin:               'System Admin',
  cfo:                 'Chief Financial Officer',
  ceo:                 'Chief Executive Officer',
  finance:             'Financial Officer',
  officer:             'Officer',
  it:                  'IT Manager',
  hr:                  'HR Officer',
  hr_manager:          'HR Manager',
  sales_rep:           'Sales Representative',
  sales_supervisor:    'Sales Supervisor',
};

export const ROLE_LABEL_TH: Record<DisplayRoleName, string> = {
  staff:               'เจ้าหน้าที่',
  accountant:          'นักบัญชี',
  account_officer:     'เจ้าหน้าที่บัญชี',
  account_supervisor:  'หัวหน้างานบัญชี',
  accounting_manager:  'ผู้จัดการบัญชี',
  supervisor:          'หัวหน้าทีม',
  head_of_department:  'หัวหน้าแผนก',
  admin:               'ผู้บริหาร',
  cfo:                 'ประธานเจ้าหน้าที่ฝ่ายการเงิน',
  ceo:                 'ประธานเจ้าหน้าที่บริหาร',
  it:                  'เจ้าหน้าที่ไอที',
  hr:                  'เจ้าหน้าที่ HR',
  hr_manager:          'ผู้จัดการ HR',
  manager:             'ผู้จัดการ',
  finance:             'เจ้าหน้าที่การเงิน',
  officer:             'เจ้าหน้าที่ระดับปฏิบัติการ',
  sales_rep:           'เซลล์',
  sales_supervisor:    'หัวหน้าทีมขาย',
};

export const ROLE_BADGE: Record<DisplayRoleName, string> = {
  staff:               'bg-emerald-500/15 text-emerald-200 border-emerald-500/40',
  accountant:          'bg-sky-500/15 text-sky-200 border-sky-500/40',
  account_officer:     'bg-cyan-500/15 text-cyan-200 border-cyan-500/40',
  account_supervisor:  'bg-indigo-500/15 text-indigo-200 border-indigo-500/40',
  accounting_manager:  'bg-amber-500/15 text-amber-200 border-amber-500/40',
  supervisor:          'bg-cyan-500/15 text-cyan-200 border-cyan-500/40',
  head_of_department:  'bg-amber-500/15 text-amber-200 border-amber-500/40',
  admin:               'bg-purple-500/15 text-purple-200 border-purple-500/40',
  cfo:                 'bg-purple-500/15 text-purple-200 border-purple-500/40',
  ceo:                 'bg-rose-500/15 text-rose-200 border-rose-500/40',
  it:                  'bg-slate-500/15 text-slate-200 border-slate-500/40',
  hr:                  'bg-pink-500/15 text-pink-200 border-pink-500/40',
  hr_manager:          'bg-fuchsia-500/15 text-fuchsia-200 border-fuchsia-500/40',
  manager:             'bg-cyan-500/15 text-cyan-200 border-cyan-500/40',
  finance:             'bg-emerald-500/15 text-emerald-200 border-emerald-500/40',
  officer:             'bg-slate-500/15 text-slate-200 border-slate-500/40',
  sales_rep:           'bg-orange-500/15 text-orange-200 border-orange-500/40',
  sales_supervisor:    'bg-amber-500/15 text-amber-200 border-amber-500/40',
};

export const ROLE_RANK: Record<DisplayRoleName, number> = {
  ceo:                 1,
  cfo:                 2,
  admin:               3,
  head_of_department:  4,
  accounting_manager:  5,
  hr_manager:          6,
  account_supervisor:  7,
  supervisor:          8,
  account_officer:     9,
  accountant:          10,
  hr:                  11,
  it:                  12,
  staff:               13,
  manager:             4,
  finance:             5,
  officer:             5,
  sales_rep:           9,
  sales_supervisor:    8,
};

export const ROLE_LEVEL: Record<DisplayRoleName, StaffLevel> = {
  staff:               5,
  accountant:          5,
  account_officer:     5,
  account_supervisor:  4,
  accounting_manager:  3,
  supervisor:          4,
  head_of_department:  3,
  admin:               2,
  cfo:                 2,
  ceo:                 1,
  it:                  5,
  hr:                  5,
  hr_manager:          3,
  manager:             3,
  finance:             5,
  officer:             5,
  sales_rep:           5,
  sales_supervisor:    4,
};

export const ROLE_DOMAIN: Record<DisplayRoleName, string> = {
  staff:               'general',
  accountant:          'finance',
  account_officer:     'finance',
  account_supervisor:  'finance',
  accounting_manager:  'finance',
  supervisor:          'general',
  head_of_department:  'general',
  admin:               'exec',
  cfo:                 'exec',
  ceo:                 'exec',
  it:                  'it',
  hr:                  'hr',
  hr_manager:          'hr',
  manager:             'general',
  finance:             'finance',
  officer:             'general',
  sales_rep:           'sales',
  sales_supervisor:    'sales',
};

export const STAFF_LEVEL_LABEL: Record<StaffLevel, string> = {
  1: 'CEO',
  2: 'C-Level',
  3: 'Manager',
  4: 'Supervisor',
  5: 'Officer',
};

export const STAFF_LEVEL_ACCENT: Record<StaffLevel, string> = {
  1: 'from-rose-500 to-pink-700',
  2: 'from-purple-500 to-fuchsia-700',
  3: 'from-amber-500 to-orange-700',
  4: 'from-cyan-500 to-sky-700',
  5: 'from-emerald-500 to-teal-700',
};

export const STAFF_LEVEL_BADGE: Record<StaffLevel, string> = {
  1: 'bg-rose-500/15 text-rose-200 border-rose-500/40',
  2: 'bg-purple-500/15 text-purple-200 border-purple-500/40',
  3: 'bg-amber-500/15 text-amber-200 border-amber-500/40',
  4: 'bg-cyan-500/15 text-cyan-200 border-cyan-500/40',
  5: 'bg-emerald-500/15 text-emerald-200 border-emerald-500/40',
};

// ── Helper functions ────────────────────────────────────────────────────────

export function staffLevelLabel(level: number | null | undefined): string {
  if (level == null) return 'Unknown';
  return STAFF_LEVEL_LABEL[level as StaffLevel] ?? `Grade ${level}`;
}

export function staffLevelAccent(level: number | null | undefined): string {
  if (level == null) return STAFF_LEVEL_ACCENT[5];
  return STAFF_LEVEL_ACCENT[level as StaffLevel] ?? STAFF_LEVEL_ACCENT[5];
}

export function staffLevelBadge(level: number | null | undefined): string {
  if (level == null) return STAFF_LEVEL_BADGE[5];
  return STAFF_LEVEL_BADGE[level as StaffLevel] ?? STAFF_LEVEL_BADGE[5];
}

export function isStaffLevel(n: unknown): n is StaffLevel {
  return typeof n === 'number' && n >= 1 && n <= 5;
}

export function getDefaultStaffLevel(_role: string): StaffLevel {
  return 5;
}

export function getEffectiveStaffLevel(input: { staff_level?: number | null; role_name?: string | null }): StaffLevel {
  if (typeof input.staff_level === 'number' && isStaffLevel(input.staff_level)) return input.staff_level;
  return 5;
}

// Deprecated — no longer role-gated; return everything.
export function getAllowedTabs(_role: string): TabName[] {
  return ['dashboard', 'approvals', 'expenses', 'reports', 'settings'];
}

export function getDefaultTab(_role: string): TabName {
  return 'dashboard';
}

export function getRoleScope(_role: string): 'self' | 'dept' | 'all' {
  return 'self';
}

export function canAccessTab(_role: string, _tab: string): boolean {
  return true;
}

export function canPerformAction(_role: string, _action: string): boolean {
  return true;
}

export function getChainStages(_role: string): string[] {
  return [];
}
