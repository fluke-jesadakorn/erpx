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
  | 'officer'
  | 'it_manager'
  | 'it_supervisor'
  | 'it_officer'
  | 'hr_supervisor'
  | 'hr_officer'
  | 'accounting_supervisor'
  | 'accounting_officer'
  | 'finance_manager'
  | 'finance_supervisor'
  | 'finance_officer';

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
  it_manager:          '💻',
  it_supervisor:       '💻',
  it_officer:          '💻',
  hr_supervisor:       '🤝',
  hr_officer:          '🤝',
  accounting_supervisor: '📊',
  accounting_officer:  '📋',
  finance_manager:     '💰',
  finance_supervisor:  '💰',
  finance_officer:     '💰',
};

export const ROLE_ACCENT: Record<DisplayRoleName, string> = {
  staff:               'from-positive-soft to-positive-strong',
  accountant:          'from-info-soft to-info-strong',
  account_officer:     'from-info-soft to-info-strong',
  account_supervisor:  'from-accent-soft to-accent-strong',
  accounting_manager:  'from-caution-soft to-caution-strong',
  supervisor:          'from-info-soft to-info-strong',
  head_of_department:  'from-caution-soft to-caution-strong',
  admin:               'from-accent-soft to-accent-strong',
  cfo:                 'from-accent-soft to-accent-strong',
  ceo:                 'from-critical-soft to-critical-strong',
  it:                  'from-info-soft to-info-strong',
  hr:                  'from-critical-soft to-critical-strong',
  hr_manager:          'from-accent-soft to-accent-strong',
  manager:             'from-info-soft to-info-strong',
  finance:             'from-positive-soft to-positive-strong',
  officer:             'from-paper-2 to-paper-3',
  sales_rep:           'from-caution-soft to-caution-strong',
  sales_supervisor:    'from-caution-soft to-caution-strong',
  it_manager:          'from-info-soft to-info-strong',
  it_supervisor:       'from-info-soft to-info-strong',
  it_officer:          'from-info-soft to-info-strong',
  hr_supervisor:       'from-accent-soft to-accent-strong',
  hr_officer:          'from-critical-soft to-critical-strong',
  accounting_supervisor: 'from-accent-soft to-accent-strong',
  accounting_officer:  'from-info-soft to-info-strong',
  finance_manager:     'from-positive-soft to-positive-strong',
  finance_supervisor:  'from-positive-soft to-positive-strong',
  finance_officer:     'from-positive-soft to-positive-strong',
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
  it_manager:          'IT Manager',
  it_supervisor:       'IT Supervisor',
  it_officer:          'IT Officer',
  hr_supervisor:       'HR Supervisor',
  hr_officer:          'HR Officer',
  accounting_supervisor: 'Accounting Supervisor',
  accounting_officer:  'Accounting Officer',
  finance_manager:     'Financial Manager',
  finance_supervisor:  'Financial Supervisor',
  finance_officer:     'Financial Officer',
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
  it_manager:          'ผู้จัดการฝ่ายเทคโนโลยีสารสนเทศ',
  it_supervisor:       'หัวหน้างานฝ่ายเทคโนโลยีสารสนเทศ',
  it_officer:          'เจ้าหน้าที่ฝ่ายเทคโนโลยีสารสนเทศ',
  hr_supervisor:       'หัวหน้างานฝ่ายทรัพยากรบุคคล',
  hr_officer:          'เจ้าหน้าที่ฝ่ายทรัพยากรบุคคล',
  accounting_supervisor: 'หัวหน้างานฝ่ายบัญชี',
  accounting_officer:  'เจ้าหน้าที่ฝ่ายบัญชี',
  finance_manager:     'ผู้จัดการฝ่ายการเงิน',
  finance_supervisor:  'หัวหน้างานฝ่ายการเงิน',
  finance_officer:     'เจ้าหน้าที่ฝ่ายการเงิน',
};

export const ROLE_BADGE: Record<DisplayRoleName, string> = {
  staff:               'bg-positive-soft text-positive-strong border-positive/40',
  accountant:          'bg-info-soft text-info-strong border-info/40',
  account_officer:     'bg-info-soft text-info-strong border-info/40',
  account_supervisor:  'bg-accent-soft text-accent-strong border-accent/40',
  accounting_manager:  'bg-caution-soft text-caution-strong border-caution/40',
  supervisor:          'bg-info-soft text-info-strong border-info/40',
  head_of_department:  'bg-caution-soft text-caution-strong border-caution/40',
  admin:               'bg-accent-soft text-accent-strong border-accent/40',
  cfo:                 'bg-accent-soft text-accent-strong border-accent/40',
  ceo:                 'bg-critical-soft text-critical-strong border-critical/40',
  it:                  'bg-info-soft text-info-strong border-info/40',
  hr:                  'bg-critical-soft text-critical-strong border-critical/40',
  hr_manager:          'bg-accent-soft text-accent-strong border-accent/40',
  manager:             'bg-info-soft text-info-strong border-info/40',
  finance:             'bg-positive-soft text-positive-strong border-positive/40',
  officer:             'bg-neutral-soft text-neutral-strong border-neutral/40',
  sales_rep:           'bg-caution-soft text-caution-strong border-caution/40',
  sales_supervisor:    'bg-caution-soft text-caution-strong border-caution/40',
  it_manager:          'bg-info-soft text-info-strong border-info/40',
  it_supervisor:       'bg-info-soft text-info-strong border-info/40',
  it_officer:          'bg-info-soft text-info-strong border-info/40',
  hr_supervisor:       'bg-accent-soft text-accent-strong border-accent/40',
  hr_officer:          'bg-critical-soft text-critical-strong border-critical/40',
  accounting_supervisor: 'bg-accent-soft text-accent-strong border-accent/40',
  accounting_officer:  'bg-info-soft text-info-strong border-info/40',
  finance_manager:     'bg-positive-soft text-positive-strong border-positive/40',
  finance_supervisor:  'bg-positive-soft text-positive-strong border-positive/40',
  finance_officer:     'bg-positive-soft text-positive-strong border-positive/40',
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
  it_manager:          3,
  it_supervisor:       4,
  it_officer:          5,
  hr_supervisor:       4,
  hr_officer:          5,
  accounting_supervisor: 4,
  accounting_officer:  5,
  finance_manager:     3,
  finance_supervisor:  4,
  finance_officer:     5,
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
  it_manager:          3,
  it_supervisor:       4,
  it_officer:          5,
  hr_supervisor:       4,
  hr_officer:          5,
  accounting_supervisor: 4,
  accounting_officer:  5,
  finance_manager:     3,
  finance_supervisor:  4,
  finance_officer:     5,
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
  it_manager:          'it',
  it_supervisor:       'it',
  it_officer:          'it',
  hr_supervisor:       'hr',
  hr_officer:          'hr',
  accounting_supervisor: 'finance',
  accounting_officer:  'finance',
  finance_manager:     'finance',
  finance_supervisor:  'finance',
  finance_officer:     'finance',
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
