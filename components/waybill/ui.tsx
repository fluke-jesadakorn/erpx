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
        card: 'border-positive bg-positive-strong text-positive-soft',
        bullet: 'bg-positive shadow-[0_0_10px_rgba(52,211,153,0.7)] text-positive-soft',
        badge: 'bg-positive text-paper border border-positive',
        badgeText: 'Done',
        title: 'text-positive-soft',
        sectionHead: 'text-positive',
        dot: 'bg-positive text-paper',
      };
    case 'active':
      return {
        card: 'border-info bg-info-strong text-ink ring-2 ring-info shadow-lg shadow-info',
        bullet: 'bg-info shadow-[0_0_14px_rgba(34,211,238,0.9)] animate-pulse text-info-soft',
        badge: 'bg-info text-ink border border-info',
        badgeText: 'Active',
        title: 'text-info-soft',
        sectionHead: 'text-info',
        dot: 'bg-info text-paper',
      };
    case 'skipped':
      return {
        card: 'border-rule bg-paper-2/50 text-mute opacity-60',
        bullet: 'bg-paper-2 text-mute',
        badge: 'bg-paper-2 text-mute border border-rule',
        badgeText: 'Optional',
        title: 'text-mute',
        sectionHead: 'text-mute',
        dot: 'bg-paper-2 text-mute',
      };
    case 'rejected':
      return {
        card: 'border-critical bg-critical-strong text-critical-soft ring-2 ring-critical',
        bullet: 'bg-critical-soft text-critical-strong border border-critical shadow-[0_0_10px_rgba(244,63,94,0.7)]',
        badge: 'bg-critical-soft text-critical-strong border border-critical border border-critical',
        badgeText: 'Rejected',
        title: 'text-critical-soft',
        sectionHead: 'text-critical',
        dot: 'bg-critical-soft text-critical-strong border border-critical',
      };
    case 'pending':
    default:
      return {
        card: 'border-rule/60 bg-paper-2/50 text-ink-2',
        bullet: 'bg-paper-2 text-mute',
        badge: 'bg-paper-2 text-mute border border-rule',
        badgeText: 'Pending',
        title: 'text-ink-2',
        sectionHead: 'text-mute',
        dot: 'bg-paper-2 text-mute',
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
    supervisor: 'bg-info text-paper border-info',
    manager: 'bg-info text-paper border-info',
    account_officer: 'bg-accent text-paper border-accent',
    account_supervisor: 'bg-accent text-paper border-accent',
    accounting_manager: 'bg-accent text-paper border-accent',
    finance: 'bg-positive text-paper border-positive',
    cfo: 'bg-accent text-paper border-accent',
    ceo: 'bg-accent text-paper border-accent',
    admin: 'bg-paper-2/20 text-ink border-rule/40',
  };
  return map[roleId ?? ''] ?? 'bg-paper-2/40 text-ink-2 border-rule';
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
    draft: { tone: 'bg-paper-2/40 text-ink-2 border-rule', en: 'draft', th: 'ร่าง', de: 'Entwurf' },
    submission: { tone: 'bg-info text-paper border-info', en: 'submission', th: 'ยื่นเรื่อง', de: 'Einreichung' },
    dept_verification: { tone: 'bg-caution-soft text-caution-strong border border-caution border-caution', en: 'dept verification', th: 'ตรวจสอบแผนก', de: 'Abteilungsprüfung' },
    dept_authorization: { tone: 'bg-caution-soft text-caution-strong border border-caution border-caution', en: 'dept authorization', th: 'อนุมัติแผนก', de: 'Abteilungsfreigabe' },
    accounting_verification: { tone: 'bg-accent text-paper border-accent', en: 'accounting verification', th: 'บัญชีตรวจ', de: 'Buchhaltungsprüfung' },
    accounting_authorization: { tone: 'bg-accent text-paper border-accent', en: 'accounting authorization', th: 'บัญชีอนุมัติ', de: 'Buchhaltungsfreigabe' },
    awaiting_disbursement: { tone: 'bg-info text-paper border-info', en: 'awaiting disbursement', th: 'พร้อมจ่าย', de: 'Auszahlung anstehend' },
    disbursed: { tone: 'bg-positive text-paper border-positive', en: 'disbursed', th: 'จ่ายแล้ว', de: 'ausgezahlt' },
    rejected: { tone: 'bg-critical-soft text-critical-strong border border-critical border-critical', en: 'rejected', th: 'ปฏิเสธ', de: 'abgelehnt' },
    completed: { tone: 'bg-positive text-paper border-positive', en: 'completed', th: 'เสร็จสิ้น', de: 'abgeschlossen' },
    open: { tone: 'bg-info text-paper border-info', en: 'open', th: 'ดำเนินการ', de: 'offen' },
  };
  const m = map[status] ?? { tone: 'bg-paper-2/40 text-ink-2 border-rule', en: status, th: status, de: status };
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