export type CurrencyCode = string;

export interface Money {
  currency: CurrencyCode;
  amount: number;
}

export interface FxAmount {
  foreign: Money;
  rateToThb: number;
  functionalThb: number;
}

export interface PostingActor {
  id: number;
  permissions: string[];
}

export interface JournalLine {
  accountCode: string;
  description: string;
  debitThb?: number;
  creditThb?: number;
  foreignAmount?: number | null;
  currencyCode?: CurrencyCode | null;
  branchId?: number;
  departmentId?: string | null;
  customerId?: number | null;
  vendorId?: number | null;
  employeeId?: number | null;
  productId?: number | null;
  warehouseId?: number | null;
  waybillId?: string | null;
  sourceDocumentType?: string | null;
  sourceDocumentId?: string | null;
  metadata?: Record<string, unknown>;
}

export interface JournalDraft {
  id?: number;
  postingDate: string;
  documentDate?: string;
  description: string;
  currencyCode?: CurrencyCode;
  fxRate?: number;
  sourceType: string;
  sourceId: string;
  sourceEventKey: string;
  branchId: number;
  waybillId?: string | null;
  attachments?: string[];
  metadata?: Record<string, unknown>;
  lines: JournalLine[];
}

export interface JournalRecord {
  id: number;
  journalNo: string | null;
  status: 'draft' | 'prepared' | 'posted' | 'void';
  postingDate: string;
  documentDate: string;
  description: string;
  currencyCode: string;
  fxRate: number;
  sourceType: string;
  sourceId: string;
  sourceEventKey: string;
  branchId: number;
  waybillId: string | null;
  preparerId: number | null;
  approverId: number | null;
  reversalOfId: number | null;
  lines: JournalLine[];
}

export interface ArDocument {
  id: number;
  customerId: number;
  documentNo: string;
  documentDate: string;
  dueDate: string;
  currencyCode: string;
  originalForeign: number;
  openForeign: number;
  originalThb: number;
  openThb: number;
  status: 'open' | 'partially_paid' | 'paid' | 'void';
}

export interface ApDocument {
  id: number;
  vendorId: number | null;
  employeeId: number | null;
  documentNo: string;
  documentDate: string;
  dueDate: string;
  currencyCode: string;
  originalForeign: number;
  openForeign: number;
  originalThb: number;
  openThb: number;
  status: 'open' | 'partially_paid' | 'paid' | 'void';
}

export interface ReportFilter {
  dateFrom: string;
  dateTo: string;
  branchId?: number | null;
  accountCode?: string | null;
}
