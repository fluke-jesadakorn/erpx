import {z} from 'zod';
export const bankCsvMappingSchema = z.object({
  name: z.string().trim().min(1).max(160),
  columns: z.object({
    externalId: z.string().min(1),
    transactionDate: z.string().min(1),
    description: z.string().optional(),
    amount: z.string().min(1),
    currency: z.string().optional(),
  }),
  dateFormat: z.enum(["YYYY-MM-DD", "DD/MM/YYYY"]).default("YYYY-MM-DD"),
});
export const bankCsvImportSchema = z.object({
  mappingId: z.string().min(1),
  bankAccountId: z.string().min(1),
  statementNumber: z.string().trim().min(1).max(100),
  statementDate: z.string().date(),
  opening: z.string(),
  closing: z.string(),
  closingFxRate: z.string().optional(),
  csv: z.string().max(5_000_000),
});
export const bankMatchGroupSchema = z
  .object({
    bankTransactionIds: z.array(z.string().min(1)).min(1).max(100),
    journalLineIds: z.array(z.string().min(1)).min(1).max(100),
    reason: z.string().trim().min(3).max(2000),
  })
  .refine(
    (v) =>
      new Set(v.bankTransactionIds).size === v.bankTransactionIds.length &&
      new Set(v.journalLineIds).size === v.journalLineIds.length,
    "Select every bank row and journal line once.",
  );
export type BankRow = {
  id: string;
  version: number;
  currency: string;
  bank_account: string;
  amount_minor: number;
  transaction_date: string;
  match_status: string;
};
export type LedgerRow = {
  id: string;
  account_id: string;
  debit_minor: number;
  credit_minor: number;
  currency: string;
  status: string;
  version: number;
  entry_date: string;
  journal_id: string;
};
export type BankAutomationQuery = {
  bankCursor?: string;
  cashCursor?: string;
  groupCursor?: string;
  chart?: string;
  q?: string;
  cashQuery?: string;
};

export type BankSourceFacts={banks:BankRow[];lines:LedgerRow[];currency:string;account:string;hash:string};
export type BankMatchGroup={version:number;status:string;source_hash:string;explanation_json:string;proposed_by:string};
export type BankMatchAllocation={id:string;bankId:string;lineId:string;amount:number};
export type BankCandidate={id:string;exact_count:number;exact_id:string|null;same_day_count:number;same_day_amount:string;same_day_ids:string};
export type BankPickerPage<T>={items:T[];nextCursor:string|null;previousCursor:string|null;hasMore:boolean};
