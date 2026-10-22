/** Old immutable captures retain their original classification and export bytes. */
export function capturedFiscalClose(source: { is_fiscal_close?: unknown; source_type?: unknown }): boolean {
  return Number(source.is_fiscal_close) === 1 || source.source_type === "fiscal_close";
}
