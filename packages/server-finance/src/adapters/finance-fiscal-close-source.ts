/** Internal journal alias only; never interpolate request input into this SQL. */
export function fiscalCloseSourceSql(alias = "j"): string {
  if (!/^[a-z][a-z0-9_]*$/i.test(alias)) throw new Error("Invalid journal alias");
  return `(${alias}.source_type='fiscal_close' OR (${alias}.source_type='book_projection' AND EXISTS (SELECT 1 FROM finance_journals fiscal_origin WHERE fiscal_origin.id=${alias}.source_id AND fiscal_origin.company_id=${alias}.company_id AND fiscal_origin.source_type='fiscal_close')))`;
}

export { capturedFiscalClose } from "@erpx/server-finance/domain/fiscal-close";
