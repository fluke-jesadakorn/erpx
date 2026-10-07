import type {CloseReconciliation} from './finance';
import type {VatCloseEvidence} from './vat-register';
export type PaymentRecord = { id: string; page_key: string; payment_number: string; kind: string; payment_date: string; currency: string; amount_minor: number; outstanding_minor: number; status: string; party_name: string | null };
export type FinanceAttention = {
  id: string;
  label: string;
  description: string;
  count: number;
  amountMinor: number | null;
  currency: string | null;
  amounts?: Array<{currency:string;amountMinor:number}>;
  href: string;
  tone: "urgent" | "attention" | "normal";
};

export type PaymentObligation = {
  id: string;
  sourceType: string;
  sourceLabel: string;
  sourceId: string;
  dueDate: string | null;
  currency: string;
  amountMinor: number;
  status: string;
  nextAction: string;
  href: string;
};

export type PaymentObligationFilters = {
  sourceType?: string;
  status?: string;
};

export type CountRow = Record<string, number | string | null>;
export type ClosePeriodRow = {
  [key: string]: string | number | null | CloseReconciliation[] | VatCloseEvidence;
  id: string;
  name: string;
  start_date: string;
  end_date: string;
  state: string;
  draft_journals: number;
  unmatched_bank: number;
  draft_invoices: number;
  open_manufacturing: number;
  pending_calculations: number;
  pending_tax: number;
  pending_source_tasks: number;
  blockers: number;
  reconciliation_ready: number;
  reconciliations: CloseReconciliation[];
  vat_reconciliation: VatCloseEvidence|null;
};
export type ClosePeriodSource = {
  [key: string]: string | number | null;
  id: string;
  name: string;
  start_date: string;
  end_date: string;
  state: string;
  draft_journals: number;
  unmatched_bank: number;
  draft_invoices: number;
  open_manufacturing: number;
  pending_calculations: number;
};

