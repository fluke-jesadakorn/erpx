// lib/waybill/labels.ts — canonical bilingual stage vocabulary for the
// Waybill UI. The new key set replaces the legacy snake_case codes.

export interface WaybillStagePip {
  key: string;
  en: string;
  th: string;
  de?: string;
  emoji: string;
  bucket: 'submission' | 'verification' | 'authorization' | 'disbursement' | 'closed';
  description_en: string;
  description_th: string;
  description_de?: string;
  thirdParty?: boolean;
  paysBefore?: boolean;
}

export interface WaybillDomainPips {
  title_en: string;
  title_th: string;
  pips: WaybillStagePip[];
}

export const EXPENSE_STAGES: WaybillDomainPips = {
  title_en: 'Expense Waybill',
  title_th: 'ใบส่งจ่าย (เบิกจ่ายพนักงาน)',
  pips: [
    {
      key: 'submission',
      en: 'Submission',
      th: 'ยื่นเบิก',
      emoji: '📤',
      bucket: 'submission',
      description_en: 'Slip uploaded · OCR auto-extracted fields',
      description_th: 'อัปโหลดสลิป · OCR ดึงข้อมูลอัตโนมัติ',
    },
    {
      key: 'dept_verification',
      en: 'Dept Verification',
      th: 'ตรวจสอบระดับแผนก',
      emoji: '👥',
      bucket: 'verification',
      description_en: 'Supervisor confirms vendor + items',
      description_th: 'หัวหน้างานยืนยันผู้ขายและรายการ',
    },
    {
      key: 'dept_authorization',
      en: 'Dept Authorization',
      th: 'อนุมัติระดับแผนก',
      emoji: '🛡️',
      bucket: 'authorization',
      description_en: 'Dept manager signs off',
      description_th: 'ผู้จัดการแผนกลงนาม',
    },
    {
      key: 'accounting_verification',
      en: 'Accounting Verification',
      th: 'บัญชีตรวจสอบ',
      emoji: '🧾',
      bucket: 'verification',
      description_en: 'Account officer line-item check',
      description_th: 'เจ้าหน้าที่บัญชีตรวจรายการ',
    },
    {
      key: 'accounting_supervision',
      en: 'Accounting Supervision',
      th: 'หน.บัญชีตรวจทาน',
      emoji: '🧮',
      bucket: 'verification',
      description_en: 'Account supervisor quality control',
      description_th: 'หัวหน้าบัญชีตรวจทาน',
    },
    {
      key: 'accounting_authorization',
      en: 'Accounting Authorization',
      th: 'ผจก.บัญชีลงนาม',
      emoji: '⚙️',
      bucket: 'authorization',
      description_en: 'Accounting manager recognises expense',
      description_th: 'ผู้จัดการบัญชีรับรองรายจ่าย',
    },
    {
      key: 'disbursement_authorization',
      en: 'Disbursement Authorization',
      th: 'อนุมัติจ่าย',
      emoji: '💰',
      bucket: 'authorization',
      description_en: 'Finance releases funds',
      description_th: 'การเงินอนุมัติจ่าย',
    },
    {
      key: 'cfo_authorization',
      en: 'CFO Authorization',
      th: 'CFO ลงนาม',
      emoji: '👑',
      bucket: 'authorization',
      description_en: 'CFO fiscal sign-off',
      description_th: 'CFO ลงนามทางการเงิน',
    },
    {
      key: 'ceo_authorization',
      en: 'CEO Authorization',
      th: 'CEO ลงนาม',
      emoji: '🦅',
      bucket: 'authorization',
      description_en: 'Auto-armed when total ≥ 200,000 THB',
      description_th: 'ขอ CEO ลงนามอัตโนมัติเมื่อยอด ≥ 200,000 บาท',
    },
    {
      key: 'awaiting_disbursement',
      en: 'Awaiting Disbursement',
      th: 'พร้อมจ่าย',
      emoji: '✅',
      bucket: 'disbursement',
      description_en: 'Approved — settlement slot',
      description_th: 'อนุมัติแล้ว — รอจ่าย',
    },
    {
      key: 'disbursed',
      en: 'Disbursed',
      th: 'จ่ายแล้ว',
      emoji: '💳',
      bucket: 'closed',
      description_en: 'Paid · slip issued · GL posted',
      description_th: 'จ่ายสำเร็จ · ออกสลิป · บันทึกบัญชี',
    },
    {
      key: 'rejected',
      en: 'Rejected',
      th: 'ปฏิเสธ',
      emoji: '❌',
      bucket: 'closed',
      description_en: 'Stopped pending resubmission',
      description_th: 'หยุดรอ ส่งใหม่ได้',
    },
  ],
};

export const PROCUREMENT_STAGES: WaybillDomainPips = {
  title_en: 'Procurement Waybill',
  title_th: 'ใบส่งจ่าย (จัดซื้อจัดจ้าง)',
  pips: [
    {
      key: 'submission',
      en: 'PR Submitted',
      th: 'ยื่น PR',
      emoji: '📝',
      bucket: 'submission',
      description_en: 'Purchase request created',
      description_th: 'ส่งคำขอซื้อ',
    },
    {
      key: 'dept_authorization',
      en: 'PR Approved',
      th: 'อนุมัติ PR',
      emoji: '🛡️',
      bucket: 'authorization',
      description_en: 'Dept manager signs',
      description_th: 'ผู้จัดการแผนกลงนาม',
    },
    {
      key: 'accounting_authorization',
      en: 'PO Issued',
      th: 'ออก PO',
      emoji: '📦',
      bucket: 'disbursement',
      description_en: 'Accounting issues PO',
      description_th: 'บัญชีออกใบสั่งซื้อ',
    },
    {
      key: 'cfo_authorization',
      en: 'PO Approved',
      th: 'อนุมัติ PO',
      emoji: '✅',
      bucket: 'authorization',
      description_en: 'PO sign-off before payment',
      description_th: 'ลงนาม PO ก่อนจ่าย',
    },
    {
      key: 'disbursed',
      en: 'Payslip',
      th: 'สลิปจ่าย',
      emoji: '💳',
      bucket: 'closed',
      description_en: 'Disbursement slip attached',
      description_th: 'แนบสลิปจ่ายเงิน',
    },
  ],
};

// Convenience: fallback label lookup by stage key.
export const SALES_STAGES: WaybillDomainPips = {
  title_en: 'Sales Order Waybill',
  title_th: 'ใบส่งจ่าย (ใบสั่งขาย)',
  pips: [
    { key: 'so_draft', en: 'Draft', th: 'ร่าง', de: 'Entwurf', emoji: '📝', bucket: 'submission', description_en: 'Composing SO', description_th: 'กำลังร่าง SO', description_de: 'SO wird erstellt' },
    { key: 'so_sales_review', en: 'Sales Review', th: 'ตรวจสอบยอดขาย', de: 'Verkaufsprüfung', emoji: '🛡️', bucket: 'authorization', description_en: 'Sales supervisor confirms', description_th: 'หัวหน้าทีมขายยืนยัน', description_de: 'Verkaufsleiter bestätigt' },
    { key: 'so_credit_check', en: 'Credit Check', th: 'ตรวจเครดิตลูกค้า', de: 'Bonitätsprüfung', emoji: '🔍', bucket: 'verification', description_en: 'Verify AR + credit limit', description_th: 'ตรวจสอบวงเงิน AR + เครดิต', description_de: 'AR + Kreditlimit prüfen' },
    { key: 'so_invoiced', en: 'Invoiced', th: 'ออกใบกำกับภาษี', de: 'Fakturiert', emoji: '🧾', bucket: 'disbursement', description_en: 'Tax Invoice issued', description_th: 'ออกใบกำกับภาษีแล้ว', description_de: 'Rechnung ausgestellt', thirdParty: true },
    { key: 'so_paid', en: 'Paid (AR Receipt)', th: 'รับชำระแล้ว', de: 'Bezahlt', emoji: '💰', bucket: 'closed', description_en: 'AR receipt attached', description_th: 'แนบสลิปรับชำระแล้ว', description_de: 'AR-Beleg angehängt', thirdParty: true },
  ],
};

export function stageLabel(
  stageKey: string,
  domain: 'expense' | 'procurement' | 'sales',
  lang: 'en' | 'th' = 'en',
): { label: string; emoji: string } {
  const set = domain === 'sales' ? SALES_STAGES : domain === 'procurement' ? PROCUREMENT_STAGES : EXPENSE_STAGES;
  const pip = set.pips.find((p) => p.key === stageKey);
  if (pip) return { label: lang === 'th' ? pip.th : pip.en, emoji: pip.emoji };
  const fallback = EXPENSE_STAGES.pips.find((p) => p.key === stageKey);
  if (fallback) return { label: lang === 'th' ? fallback.th : fallback.en, emoji: fallback.emoji };
  return { label: stageKey, emoji: '📄' };
}

export const WAYBILL_LABELS = EXPENSE_STAGES;
