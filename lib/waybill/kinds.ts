// lib/waybill/kinds.ts
//
// Taxonomy for documents attached to a Waybill at a given stage.
// Allowed kinds per stage are enforced by `KINDS_ALLOWED_AT_STAGE`;
// all kinds are also enforced at the DB level (see migration
// 2026-07-10-A-waybill-attachments).

import type { WaybillStagePip } from './labels';
import { EXPENSE_STAGES, PROCUREMENT_STAGES } from './labels';

export type WaybillAttachmentKind =
  | 'slip'
  | 'pr_doc'
  | 'po_doc'
  | 'payment_receipt'
  | 'signoff_memo'
  | 'invoice'
  | 'wht_cert'
  | 'photo'
  | 'memo'
  | 'other';

export interface WaybillKindMeta {
  en: string;
  th: string;
  de: string;
  emoji: string;
}

export const WAYBILL_KINDS: Record<WaybillAttachmentKind, WaybillKindMeta> = {
  slip:            { en: 'Slip / receipt',        th: 'ใบเสร็จ',                  de: 'Beleg',                   emoji: '🧾' },
  pr_doc:          { en: 'PR document',           th: 'เอกสาร PR',                de: 'PR-Dokument',             emoji: '📄' },
  po_doc:          { en: 'PO document',           th: 'เอกสาร PO',                de: 'PO-Dokument',             emoji: '📎' },
  payment_receipt: { en: 'Payment receipt',       th: 'หลักฐานจ่ายเงิน',           de: 'Zahlungsbeleg',           emoji: '💸' },
  signoff_memo:    { en: 'Sign-off memo',         th: 'หนังสืออนุมัติ',            de: 'Genehmigungsschreiben',   emoji: '🛡️' },
  invoice:         { en: 'Tax invoice',           th: 'ใบกำกับภาษี',              de: 'Steuerrechnung',          emoji: '🧮' },
  wht_cert:        { en: 'WHT certificate',       th: 'หนังสือรับรองหัก ณ ที่จ่าย', de: 'Quellensteuerbescheinigung', emoji: '📑' },
  photo:           { en: 'Photo evidence',        th: 'ภาพประกอบ',                de: 'Foto',                    emoji: '🖼' },
  memo:            { en: 'Internal memo',         th: 'บันทึกภายใน',              de: 'Internes Memo',           emoji: '📝' },
  other:           { en: 'Other',                 th: 'อื่น ๆ',                   de: 'Sonstiges',               emoji: '📄' },
};

export const WAYBILL_KIND_ORDER: WaybillAttachmentKind[] = [
  'slip', 'pr_doc', 'po_doc', 'payment_receipt',
  'signoff_memo', 'invoice', 'wht_cert', 'photo', 'memo', 'other',
];

const STAFF: WaybillAttachmentKind[] = ['slip', 'invoice', 'photo', 'other', 'memo'];
const SUPERVISOR: WaybillAttachmentKind[] = ['photo', 'memo', 'other'];
const MANAGER: WaybillAttachmentKind[] = ['signoff_memo', 'memo', 'other'];
const ACCOUNTANT: WaybillAttachmentKind[] = ['invoice', 'wht_cert', 'other', 'memo'];
const ACCOUNT_SUP: WaybillAttachmentKind[] = ['wht_cert', 'memo', 'other'];
const ACCT_MGR: WaybillAttachmentKind[] = ['signoff_memo', 'memo', 'other'];
const FINANCE: WaybillAttachmentKind[] = ['payment_receipt', 'photo', 'memo', 'other'];
const CFO: WaybillAttachmentKind[] = ['signoff_memo', 'memo'];
const CEO: WaybillAttachmentKind[] = ['signoff_memo'];
const ALL: WaybillAttachmentKind[] = [...WAYBILL_KIND_ORDER];

export const KINDS_ALLOWED_AT_STAGE: Record<string, WaybillAttachmentKind[]> = {
  submission:                    STAFF,
  dept_verification:             SUPERVISOR,
  dept_authorization:            MANAGER,
  accounting_verification:       ACCOUNTANT,
  accounting_supervision:        ACCOUNT_SUP,
  accounting_authorization:      ACCT_MGR,
  disbursement_authorization:    FINANCE,
  cfo_authorization:             CFO,
  ceo_authorization:             CEO,
  awaiting_disbursement:         FINANCE,
  disbursed:                     [],
  rejected:                      [],
};

export function pipsForKindLayout(domain: 'expense' | 'procurement'): WaybillStagePip[] {
  return domain === 'procurement' ? PROCUREMENT_STAGES.pips : EXPENSE_STAGES.pips;
}

export function allowedKindsFor(stage: string): WaybillAttachmentKind[] {
  return KINDS_ALLOWED_AT_STAGE[stage] ?? ALL;
}

export function isAllowedKind(stage: string, kind: WaybillAttachmentKind): boolean {
  return KINDS_ALLOWED_AT_STAGE[stage]?.includes(kind) ?? false;
}
