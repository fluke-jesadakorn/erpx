import { receiptNetAmount } from "@erpx/feature-people_payroll/lib/expense-ai";
export type Citation = { attachmentId: string; page: number };
export type AiDraftFields = Record<string, { value: string; proposalId: string; citations?: Citation[] }>;
export type RecoverableDraft<T> = { fields: T; baseline: string; ai: AiDraftFields; touched?: string[] };
export function mergeAiDraft<T extends Record<string, string>>(previous: RecoverableDraft<T>, baseline: string, event: {
  fields: { name: string; value: string | null; citations?: Citation[] }[];
  overwrite: boolean; proposalId?: string | null; invalidate?: boolean;
}): RecoverableDraft<T> {
  const fields = { ...previous.fields }, ai = { ...previous.ai };
  if (event.invalidate) {
    if (!Object.values(ai).some(field => field.proposalId !== event.proposalId)) return previous;
    const current = JSON.parse(baseline) as T;
    for (const name of Object.keys(ai)) if (ai[name].proposalId !== event.proposalId) {
      if (fields[name] === ai[name].value) fields[name as keyof T] = current[name as keyof T];
      delete ai[name];
    }
  } else if (event.proposalId) {
    const total = event.fields.find(field => field.name === "receiptTotal")?.value;
    for (const suggestion of event.fields) {
      const name = suggestion.name as keyof T;
      const value = name === "amount" && total && Object.hasOwn(fields, "tax")
        ? receiptNetAmount(total, previous.fields.tax) : suggestion.value;
      if (Object.hasOwn(fields, name) && value !== null &&
        (event.overwrite || (!fields[name] && !previous.touched?.includes(String(name))))) {
        fields[name] = value as T[keyof T];
        ai[String(name)] = { value, proposalId: event.proposalId, citations: suggestion.citations };
      }
    }
  }
  return { ...previous, fields, ai };
}
