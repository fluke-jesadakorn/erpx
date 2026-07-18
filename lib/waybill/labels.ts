export type WaybillBucket = 'submission' | 'verification' | 'authorization' | 'disbursement' | 'closed';

export interface WaybillStagePip {
  key: string;
  label: string;
  description: string;
  en: string;
  th: string;
  de?: string;
  description_en: string;
  description_th: string;
  description_de?: string;
  emoji: string;
  bucket: WaybillBucket;
  thirdParty?: boolean;
  paysBefore?: boolean;
}

export interface WaybillDomainPips {
  title: string;
  title_en: string;
  title_th: string;
  pips: WaybillStagePip[];
}

function pip(
  key: string,
  label: string,
  description: string,
  emoji: string,
  bucket: WaybillBucket,
  options: Pick<WaybillStagePip, 'thirdParty' | 'paysBefore'> = {},
): WaybillStagePip {
  return {
    key,
    label,
    description,
    en: label,
    th: label,
    de: label,
    description_en: description,
    description_th: description,
    description_de: description,
    emoji,
    bucket,
    ...options,
  };
}

export const EXPENSE_LABEL = 'waybill.domain.expense';
export const PROCUREMENT_LABEL = 'waybill.domain.procurement';
export const SALES_LABEL = 'waybill.domain.sales';

export const EXPENSE_STAGES: WaybillDomainPips = {
  title: EXPENSE_LABEL,
  title_en: EXPENSE_LABEL,
  title_th: EXPENSE_LABEL,
  pips: [
    pip('submission', 'waybill.stage.submission', 'waybill.stage.submissionDescription', '📤', 'submission'),
    pip('department_approval', 'waybill.stage.departmentApproval', 'waybill.stage.departmentApprovalDescription', '🛡️', 'authorization'),
    pip('accounting_review', 'waybill.stage.accountingReview', 'waybill.stage.accountingReviewDescription', '🧾', 'verification'),
    pip('accounting_approval', 'waybill.stage.accountingApproval', 'waybill.stage.accountingApprovalDescription', '⚖️', 'authorization'),
    pip('executive_approval', 'waybill.stage.executiveApproval', 'waybill.stage.executiveApprovalDescription', '👑', 'authorization'),
    pip('payment', 'waybill.stage.payment', 'waybill.stage.paymentDescription', '💳', 'disbursement'),
    pip('settlement', 'waybill.stage.settlement', 'waybill.stage.settlementDescription', '📚', 'closed'),
  ],
};

export const PROCUREMENT_STAGES: WaybillDomainPips = {
  title: PROCUREMENT_LABEL,
  title_en: PROCUREMENT_LABEL,
  title_th: PROCUREMENT_LABEL,
  pips: [
    pip('submission', 'waybill.stage.prSubmitted', 'waybill.stage.prSubmittedDescription', '📝', 'submission'),
    pip('dept_authorization', 'waybill.stage.prApproved', 'waybill.stage.prApprovedDescription', '🛡️', 'authorization'),
    pip('accounting_authorization', 'waybill.stage.poIssued', 'waybill.stage.poIssuedDescription', '📦', 'disbursement'),
    pip('cfo_authorization', 'waybill.stage.poApproved', 'waybill.stage.poApprovedDescription', '✅', 'authorization'),
    pip('disbursed', 'waybill.stage.payslip', 'waybill.stage.payslipDescription', '💳', 'closed'),
  ],
};

export const SALES_STAGES: WaybillDomainPips = {
  title: SALES_LABEL,
  title_en: SALES_LABEL,
  title_th: SALES_LABEL,
  pips: [
    pip('so_draft', 'waybill.stage.soDraft', 'waybill.stage.soDraftDescription', '📝', 'submission'),
    pip('so_sales_review', 'waybill.stage.soSalesReview', 'waybill.stage.soSalesReviewDescription', '🛡️', 'authorization'),
    pip('so_dept_approval', 'waybill.stage.soDeptApproval', 'waybill.stage.soDeptApprovalDescription', '🏛️', 'authorization'),
    pip('so_credit_check', 'waybill.stage.soCreditCheck', 'waybill.stage.soCreditCheckDescription', '🔍', 'verification'),
    pip('so_invoiced', 'waybill.stage.soInvoiced', 'waybill.stage.soInvoicedDescription', '🧾', 'disbursement', { thirdParty: true }),
    pip('so_paid', 'waybill.stage.soPaid', 'waybill.stage.soPaidDescription', '💰', 'closed', { thirdParty: true }),
  ],
};

export function stageLabel(
  stageKey: string,
  domain: 'expense' | 'procurement' | 'sales',
  _lang: 'en' | 'th' | 'de' = 'en',
): { label: string; emoji: string } {
  const set = domain === 'sales' ? SALES_STAGES : domain === 'procurement' ? PROCUREMENT_STAGES : EXPENSE_STAGES;
  const pip = set.pips.find((p) => p.key === stageKey);
  if (pip) return { label: pip.label, emoji: pip.emoji };
  const fallback = EXPENSE_STAGES.pips.find((p) => p.key === stageKey);
  if (fallback) return { label: fallback.label, emoji: fallback.emoji };
  return { label: stageKey, emoji: '📄' };
}

export const WAYBILL_LABELS = EXPENSE_STAGES;
