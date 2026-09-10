import { z } from "zod";
import { decimalToMinor, minorToDecimal } from "./finance-money";
export const expenseAiFields = [
  "expenseDate",
  "merchant",
  "receiptReference",
  "businessPurpose",
  "currency",
  "amount",
  "receiptTotal",
  "tax",
  "categoryAccountId",
  "entryDate",
  "periodId",
  "note",
  "date",
  "reference",
] as const;
export const expenseAiResultSchema = z
  .object({
    summary: z.string().max(4000),
    fields: z
      .array(
        z
          .object({
            name: z.enum(expenseAiFields),
            value: z.string().max(2000).nullable(),
            citations: z
              .array(
                z.object({
                  attachmentId: z.string().max(160),
                  page: z.number().int().min(1).max(20),
                }),
              )
              .min(1)
              .max(20),
          })
          .strict(),
      )
      .max(15),
    missing: z.array(z.string().max(300)).max(20),
    conflicts: z.array(z.string().max(500)).max(20),
  })
  .strict();
export type ExpenseAiResult = z.infer<typeof expenseAiResultSchema>;
export type ExpenseAiSuggestionEvent = {
  expenseId: string;
  paymentId: string | null;
  fields: ExpenseAiResult["fields"];
  overwrite: boolean;
  proposalId?: string | null;
  invalidate?: boolean;
};
export const expenseAiFieldLabels: Record<
  (typeof expenseAiFields)[number],
  string
> = {
  expenseDate: "Expense date",
  merchant: "Merchant",
  receiptReference: "Receipt number",
  businessPurpose: "Business purpose",
  currency: "Currency",
  amount: "Net amount / payment amount",
  receiptTotal: "Receipt total (including tax)",
  tax: "Recoverable tax",
  categoryAccountId: "Expense category",
  entryDate: "Posting date",
  periodId: "Accounting period",
  note: "Accounting note",
  date: "Payment date",
  reference: "Bank reference",
};

/** Net excludes only tax explicitly recorded as recoverable, not printed VAT. */
export function receiptNetAmount(total: string, recoverableTax = ""): string | null {
  try {
    if (!/^\d+(\.\d{1,2})?$/.test(total) || (recoverableTax && !/^\d+(\.\d{1,2})?$/.test(recoverableTax))) return null;
    const gross = decimalToMinor(total), tax = decimalToMinor(recoverableTax || "0");
    return gross > 0 && tax >= 0 && tax <= gross ? minorToDecimal(gross - tax) : null;
  } catch { return null; }
}

export function normalizeDraftReceiptAmounts(result: ExpenseAiResult, recoverableTax = "") {
  const total = result.fields.find(field => field.name === "receiptTotal");
  const amount = result.fields.find(field => field.name === "amount");
  const value = total?.value ? receiptNetAmount(total.value, recoverableTax) : null;
  for (const field of result.fields) if (field.name === "tax") field.value = null;
  if (amount) { amount.value = value; if (total) amount.citations = total.citations; }
  else if (total) result.fields.push({ name: "amount", value, citations: total.citations });
  if (value !== null) result.missing = result.missing.filter(name => name !== "amount");
  return result;
}
