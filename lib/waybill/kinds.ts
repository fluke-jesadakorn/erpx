import type { WaybillStagePip } from './labels';
import { EXPENSE_STAGES, PROCUREMENT_STAGES } from './labels';

export type WaybillAttachmentKind =
  | 'slip'
  | 'pr_doc'
  | 'po_doc'
  | 'expense_voucher'
  | 'payment_slip'
  | 'payment_receipt'
  | 'signoff_memo'
  | 'invoice'
  | 'wht_cert'
  | 'photo'
  | 'memo'
  | 'other';

export interface WaybillKindMeta {
  id: string;
  en: string;
  th: string;
  de: string;
  emoji: string;
}

function meta(id: string, emoji: string): WaybillKindMeta {
  return { id, en: id, th: id, de: id, emoji };
}

export const WAYBILL_KINDS: Record<WaybillAttachmentKind, WaybillKindMeta> = {
  slip: meta('waybill.attachment.slip', '🧾'),
  pr_doc: meta('waybill.attachment.prDocument', '📄'),
  po_doc: meta('waybill.attachment.poDocument', '📎'),
  expense_voucher: meta('waybill.attachment.expenseVoucher', '📄'),
  payment_slip: meta('waybill.attachment.paymentSlip', '💸'),
  payment_receipt: meta('waybill.attachment.paymentReceipt', '💸'),
  signoff_memo: meta('waybill.attachment.signoffMemo', '🛡️'),
  invoice: meta('waybill.attachment.invoice', '🧮'),
  wht_cert: meta('waybill.attachment.whtCertificate', '📑'),
  photo: meta('waybill.attachment.photo', '🖼'),
  memo: meta('waybill.attachment.memo', '📝'),
  other: meta('waybill.attachment.other', '📄'),
};

export const WAYBILL_KIND_ORDER: WaybillAttachmentKind[] = [
  'slip', 'pr_doc', 'po_doc', 'expense_voucher', 'payment_slip', 'payment_receipt',
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
  department_approval: MANAGER,
  accounting_review: ACCOUNTANT,
  accounting_approval: ACCT_MGR,
  executive_approval: CFO,
  payment: ['po_doc', 'expense_voucher', 'payment_slip', 'payment_receipt', 'photo', 'memo', 'other'],
  settlement: ACCOUNTANT,
  submission: STAFF,
  dept_verification: SUPERVISOR,
  dept_authorization: MANAGER,
  accounting_verification: ACCOUNTANT,
  accounting_supervision: ACCOUNT_SUP,
  accounting_authorization: ACCT_MGR,
  disbursement_authorization: FINANCE,
  cfo_authorization: CFO,
  ceo_authorization: CEO,
  awaiting_disbursement: FINANCE,
  disbursed: [],
  rejected: [],
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
