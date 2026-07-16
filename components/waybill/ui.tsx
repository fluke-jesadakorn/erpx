import React from 'react';
import type { PipState } from '@/waybill/derive';
import type { SecondaryLocale } from '@/server/locale';

function pick(en: string, th: string | undefined, de: string | undefined, locale: SecondaryLocale): string {
  if (locale === 'th') return th ?? en;
  if (locale === 'de') return de ?? en;
  return en;
}

export const ROLE_DISPLAY_EN: Record<string, string> = {
  supervisor: 'Supervisor',
  manager: 'Manager',
  account_officer: 'Accounting Officer',
  account_supervisor: 'Accounting Supervisor',
  accounting_manager: 'Accounting Manager',
  finance: 'Finance',
  cfo: 'CFO',
  ceo: 'CEO',
  admin: 'Admin',
};

export const ROLE_DISPLAY_TH: Record<string, string> = {
  supervisor: 'หัวหน้าทีม',
  manager: 'ผู้จัดการ',
  account_officer: 'เจ้าหน้าที่บัญชี',
  account_supervisor: 'ห.บัญชี',
  accounting_manager: 'ผจก.บัญชี',
  finance: 'การเงิน',
  cfo: 'CFO',
  ceo: 'CEO',
  admin: 'ผู้บริหาร',
};

export const ROLE_DISPLAY_DE: Record<string, string> = {
  supervisor: 'Vorgesetzter',
  manager: 'Manager',
  account_officer: 'Buchhalter',
  account_supervisor: 'Buchhaltungsleiter',
  accounting_manager: 'Buchhaltungsmanager',
  finance: 'Finanzen',
  cfo: 'CFO',
  ceo: 'CEO',
  admin: 'Administrator',
};

export function roleDisplay(roleKey: string | null | undefined, lang: 'th' | 'de' | 'en'): string {
  if (!roleKey) return '—';
  const map = lang === 'th' ? ROLE_DISPLAY_TH : lang === 'de' ? ROLE_DISPLAY_DE : ROLE_DISPLAY_EN;
  return map[roleKey] ?? roleKey;
}

export function roleDisplayBi(
  roleKey: string | null | undefined,
  locale: SecondaryLocale,
): string {
  if (!roleKey) return '—';
  return pick(roleDisplay(roleKey, 'en'), ROLE_DISPLAY_TH[roleKey], ROLE_DISPLAY_DE[roleKey], locale);
}

export function fmtTs(d: Date | string | null | undefined, lang: 'th' | 'de' | 'en'): string {
  if (d == null) return '—';
  const date = typeof d === 'string' ? new Date(d) : d;
  if (!(date instanceof Date) || Number.isNaN(date.getTime())) return '—';
  const locale = lang === 'th' ? 'th-TH' : lang === 'de' ? 'de-DE' : 'en-GB';
  return date.toLocaleString(locale, {
    day: '2-digit',
    month: 'short',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  });
}

export const EVENT_KIND_LABEL_EN: Record<string, string> = {
  created: 'created',
  submitted: 'submitted',
  advanced: 'advanced',
  rejected: 'rejected',
  corrected: 'corrected',
  settled: 'settled',
  'posted-to-gl': 'posted to GL',
  'gl-confirmed': 'gl confirmed',
  'slip-attached': 'slip attached',
  attached: 'attached',
  'signed-off': 'signed off',
  reversed: 'reversed',
  'authorization-overridden': 'auth overridden',
  resubmitted: 'resubmitted',
  superseded: 'superseded',
};

export function eventKindLabel(kind: string, lang: 'th' | 'de' | 'en'): string {
  const en = EVENT_KIND_LABEL_EN[kind] ?? kind;
  if (lang === 'th') {
    if (kind === 'advanced') return 'เดินหน้า';
    if (kind === 'rejected') return 'ปฏิเสธ';
    if (kind === 'submitted') return 'ส่งเรื่อง';
    if (kind === 'created') return 'สร้างเอกสาร';
    if (kind === 'settled') return 'จ่ายแล้ว';
    if (kind === 'attached' || kind === 'slip-attached') return 'แนบสลิป';
    if (kind === 'resubmitted') return 'ส่งใหม่';
    if (kind === 'posted-to-gl') return 'บันทึกบัญชี';
    if (kind === 'gl-confirmed') return 'ยืนยันบัญชี';
    return en;
  }
  if (lang === 'de') {
    if (kind === 'advanced') return 'fortgeschritten';
    if (kind === 'rejected') return 'abgelehnt';
    if (kind === 'submitted') return 'eingereicht';
    if (kind === 'created') return 'erstellt';
    if (kind === 'settled') return 'abgerechnet';
    if (kind === 'attached' || kind === 'slip-attached') return 'Beleg angehängt';
    if (kind === 'resubmitted') return 'erneut eingereicht';
    if (kind === 'posted-to-gl') return 'ins HB gebucht';
    if (kind === 'gl-confirmed') return 'HB bestätigt';
    if (kind === 'corrected') return 'korrigiert';
    if (kind === 'signed-off') return 'freigegeben';
    if (kind === 'reversed') return 'storniert';
    if (kind === 'authorization-overridden') return 'Freigabe überschrieben';
    if (kind === 'superseded') return 'ersetzt';
    if (kind === 'pr-created') return 'Bestellanfrage erstellt';
    if (kind === 'po-issued') return 'Bestellung ausgestellt';
    if (kind === 'posted-to-gl-accrual') return 'Rückstellung ins HB gebucht';
    if (kind === 'gl-confirmed-accrual') return 'Rückstellung HB bestätigt';
    if (kind === 'posted-to-gl-settlement') return 'Auszahlung ins HB gebucht';
    if (kind === 'gl-confirmed-settlement') return 'Auszahlung HB bestätigt';
    if (kind === 'created-draft-gl-accrual') return 'HB-Entwurf Rückstellung erstellt';
    if (kind === 'created-draft-gl-settlement') return 'HB-Entwurf Auszahlung erstellt';
    if (kind === 'so-created') return 'Auftrag erstellt';
    if (kind === 'so-submitted') return 'Auftrag eingereicht';
    if (kind === 'so-auto-approved') return 'Auftrag automatisch genehmigt';
    if (kind === 'so-reviewed') return 'Auftrag geprüft';
    if (kind === 'so-credit-checked') return 'Bonität geprüft';
    if (kind === 'so-invoiced') return 'fakturiert';
    if (kind === 'so-paid') return 'bezahlt';
    if (kind === 'so-rejected') return 'Auftrag abgelehnt';
    if (kind === 'posted-to-gl-sales-vat') return 'USt ins HB gebucht';
    if (kind === 'gl-confirmed-sales-vat') return 'USt HB bestätigt';
    if (kind === 'posted-to-gl-sales-accrual') return 'Verkauf-Rückstellung ins HB gebucht';
    if (kind === 'gl-confirmed-sales-accrual') return 'Verkauf-Rückstellung HB bestätigt';
    if (kind === 'posted-to-gl-sales-settlement') return 'Verkauf-Auszahlung ins HB gebucht';
    if (kind === 'gl-confirmed-sales-settlement') return 'Verkauf-Auszahlung HB bestätigt';
    if (kind === 'created-draft-gl-sales-vat') return 'HB-Entwurf USt erstellt';
    if (kind === 'created-draft-gl-sales-accrual') return 'HB-Entwurf Verkauf-Rückstellung erstellt';
    if (kind === 'created-draft-gl-sales-settlement') return 'HB-Entwurf Verkauf-Auszahlung erstellt';
    return en;
  }
  return en;
}

export function eventKindLabelBi(kind: string, locale: SecondaryLocale): string {
  return pick(eventKindLabel(kind, 'en'), eventKindLabel(kind, 'th'), eventKindLabel(kind, 'de'), locale);
}

export const PIP_BADGE_EN: Record<PipState, string> = {
  passed: 'Done',
  active: 'Active',
  pending: 'Pending',
  rejected: 'Rejected',
  skipped: 'Optional',
};

export const PIP_BADGE_TH: Record<PipState, string> = {
  passed: 'ผ่าน',
  active: 'กำลังดำเนินการ',
  pending: 'รอ',
  rejected: 'ปฏิเสธ',
  skipped: 'ไม่บังคับ',
};

export const PIP_BADGE_DE: Record<PipState, string> = {
  passed: 'Erledigt',
  active: 'Aktiv',
  pending: 'Ausstehend',
  rejected: 'Abgelehnt',
  skipped: 'Optional',
};

export function pipBadge(state: PipState, lang: 'th' | 'de' | 'en'): string {
  if (lang === 'th') return PIP_BADGE_TH[state];
  if (lang === 'de') return PIP_BADGE_DE[state];
  return PIP_BADGE_EN[state];
}

export function pipBadgeBi(state: PipState, locale: SecondaryLocale): string {
  return pick(PIP_BADGE_EN[state], PIP_BADGE_TH[state], PIP_BADGE_DE[state], locale);
}

export const PIP_GLYPH: Record<PipState, string> = {
  passed: '✓',
  active: '◉',
  pending: '○',
  rejected: '✗',
  skipped: '—',
};

export interface PipTone {
  card: string;
  bullet: string;
  badge: string;
  badgeText: string;
  title: string;
  sectionHead: string;
  dot: string;
}

export function toneForPip(state: PipState): PipTone {
  switch (state) {
    case 'passed':
      return {
        card: 'border-emerald-500/40 bg-emerald-950/30 text-emerald-100',
        bullet: 'bg-emerald-400 shadow-[0_0_10px_rgba(52,211,153,0.7)] text-emerald-200',
        badge: 'bg-emerald-500/20 text-emerald-200 border border-emerald-400/40',
        badgeText: 'Done',
        title: 'text-emerald-200',
        sectionHead: 'text-emerald-300/80',
        dot: 'bg-emerald-400 text-emerald-100',
      };
    case 'active':
      return {
        card: 'border-cyan-400/80 bg-cyan-950/40 text-white ring-2 ring-cyan-400/70 shadow-lg shadow-cyan-500/20',
        bullet: 'bg-cyan-300 shadow-[0_0_14px_rgba(34,211,238,0.9)] animate-pulse text-cyan-100',
        badge: 'bg-cyan-400 text-slate-950 border border-cyan-300',
        badgeText: 'Active',
        title: 'text-cyan-200',
        sectionHead: 'text-cyan-300/80',
        dot: 'bg-cyan-400 text-cyan-100',
      };
    case 'skipped':
      return {
        card: 'border-slate-900 bg-slate-950/40 text-slate-700 opacity-60',
        bullet: 'bg-slate-800 text-slate-600',
        badge: 'bg-slate-800 text-slate-600 border border-slate-700',
        badgeText: 'Optional',
        title: 'text-slate-700',
        sectionHead: 'text-slate-700',
        dot: 'bg-slate-800 text-slate-600',
      };
    case 'rejected':
      return {
        card: 'border-rose-500/60 bg-rose-950/40 text-rose-100 ring-2 ring-rose-500/50',
        bullet: 'bg-rose-400 text-rose-100 shadow-[0_0_10px_rgba(244,63,94,0.7)]',
        badge: 'bg-rose-500/20 text-rose-200 border border-rose-400/40',
        badgeText: 'Rejected',
        title: 'text-rose-200',
        sectionHead: 'text-rose-300/80',
        dot: 'bg-rose-400 text-rose-100',
      };
    case 'pending':
    default:
      return {
        card: 'border-slate-800/60 bg-slate-950/40 text-slate-400',
        bullet: 'bg-slate-700 text-slate-500',
        badge: 'bg-slate-800 text-slate-500 border border-slate-700',
        badgeText: 'Pending',
        title: 'text-slate-400',
        sectionHead: 'text-slate-500',
        dot: 'bg-slate-700 text-slate-500',
      };
  }
}

export function initialsOf(name: string | null | undefined): string {
  if (!name) return '?';
  const parts = name.trim().split(/\s+/).slice(0, 2);
  return parts.map((p) => p[0]?.toUpperCase() ?? '').join('') || '?';
}

export function roleAccent(roleId: string | null | undefined): string {
  const map: Record<string, string> = {
    supervisor: 'bg-cyan-500/20 text-cyan-200 border-cyan-400/40',
    manager: 'bg-cyan-500/20 text-cyan-200 border-cyan-400/40',
    account_officer: 'bg-indigo-500/20 text-indigo-200 border-indigo-400/40',
    account_supervisor: 'bg-indigo-500/20 text-indigo-200 border-indigo-400/40',
    accounting_manager: 'bg-indigo-500/20 text-indigo-200 border-indigo-400/40',
    finance: 'bg-emerald-500/20 text-emerald-200 border-emerald-500/40',
    cfo: 'bg-fuchsia-500/20 text-fuchsia-200 border-fuchsia-400/40',
    ceo: 'bg-fuchsia-500/20 text-fuchsia-200 border-fuchsia-400/40',
    admin: 'bg-slate-500/20 text-slate-200 border-slate-400/40',
  };
  return map[roleId ?? ''] ?? 'bg-slate-700/40 text-slate-300 border-slate-600';
}

export function fmtMoney(n: string | number | null, currency = 'THB'): string {
  if (n == null) return '—';
  const v = typeof n === 'string' ? parseFloat(n) : n;
  if (!Number.isFinite(v)) return String(n);
  return `${v.toLocaleString('th-TH', { minimumFractionDigits: 2, maximumFractionDigits: 2 })} ${currency}`;
}

export function fmtDate(d: Date | string | null): string {
  if (!d) return '—';
  const date = typeof d === 'string' ? new Date(d) : d;
  if (Number.isNaN(date.getTime())) return '—';
  return date.toISOString().slice(0, 10);
}

export function fmtSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}

export function statusPill(status: string, lang: 'th' | 'de' | 'en'): React.ReactElement {
  const map: Record<string, { tone: string; en: string; th: string; de: string }> = {
    draft: { tone: 'bg-slate-700/40 text-slate-300 border-slate-600', en: 'draft', th: 'ร่าง', de: 'Entwurf' },
    submission: { tone: 'bg-cyan-500/20 text-cyan-200 border-cyan-500/40', en: 'submission', th: 'ยื่นเรื่อง', de: 'Einreichung' },
    dept_verification: { tone: 'bg-amber-500/15 text-amber-200 border-amber-500/40', en: 'dept verification', th: 'ตรวจสอบแผนก', de: 'Abteilungsprüfung' },
    dept_authorization: { tone: 'bg-amber-500/15 text-amber-200 border-amber-500/40', en: 'dept authorization', th: 'อนุมัติแผนก', de: 'Abteilungsfreigabe' },
    accounting_verification: { tone: 'bg-indigo-500/15 text-indigo-200 border-indigo-500/40', en: 'accounting verification', th: 'บัญชีตรวจ', de: 'Buchhaltungsprüfung' },
    accounting_authorization: { tone: 'bg-indigo-500/15 text-indigo-200 border-indigo-500/40', en: 'accounting authorization', th: 'บัญชีอนุมัติ', de: 'Buchhaltungsfreigabe' },
    awaiting_disbursement: { tone: 'bg-cyan-500/15 text-cyan-200 border-cyan-500/40', en: 'awaiting disbursement', th: 'พร้อมจ่าย', de: 'Auszahlung anstehend' },
    disbursed: { tone: 'bg-emerald-500/20 text-emerald-200 border-emerald-500/40', en: 'disbursed', th: 'จ่ายแล้ว', de: 'ausgezahlt' },
    rejected: { tone: 'bg-rose-500/15 text-rose-200 border-rose-500/40', en: 'rejected', th: 'ปฏิเสธ', de: 'abgelehnt' },
    completed: { tone: 'bg-emerald-500/20 text-emerald-200 border-emerald-500/40', en: 'completed', th: 'เสร็จสิ้น', de: 'abgeschlossen' },
    open: { tone: 'bg-cyan-500/20 text-cyan-200 border-cyan-500/40', en: 'open', th: 'ดำเนินการ', de: 'offen' },
  };
  const m = map[status] ?? { tone: 'bg-slate-700/40 text-slate-300 border-slate-600', en: status, th: status, de: status };
  const locale: SecondaryLocale = lang === 'de' ? 'de' : 'th';
  return (
    <span className={`rounded-full border px-2 py-0.5 text-xs font-mono font-bold uppercase ${m.tone}`}>
      {pick(m.en, m.th, m.de, locale)}
    </span>
  );
}

export function payloadStringify(payload: unknown): string {
  try {
    return JSON.stringify(payload, null, 2);
  } catch {
    return String(payload);
  }
}