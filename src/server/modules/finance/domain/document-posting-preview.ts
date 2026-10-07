export type PostingPreviewRow = Record<string, string | number | null>;
export type PostingPreviewLine = { id: string; debit: number; credit: number };
export type PostingPreviewConfiguration = { inventory_account_id: string; variance_account_id: string };
export type DocumentPostingPreview =
  | { state: "voided" }
  | { state: "configuration-required" }
  | { state: "ready"; posted: boolean; accounts: PostingPreviewRow[]; lines: PostingPreviewLine[]; debit: number; credit: number };

export function invoicePreviewLines(row: PostingPreviewRow): PostingPreviewLine[] {
  const payable = row.kind === "payable", net = Number(row.subtotal_minor), tax = Number(row.tax_minor), total = Number(row.total_minor);
  return [
    { id: String(row.debit_account_id), debit: payable ? net : total, credit: 0 },
    { id: String(row.credit_account_id), debit: 0, credit: payable ? total : net },
    ...(tax ? [{ id: String(row.tax_account_id), debit: payable ? tax : 0, credit: payable ? 0 : tax }] : []),
  ];
}

export function inventoryPreviewLines(config: PostingPreviewConfiguration, deltas: { delta: number }[]): PostingPreviewLine[] {
  const gain = deltas.reduce((sum, line) => sum + Math.max(line.delta, 0), 0);
  const loss = deltas.reduce((sum, line) => sum + Math.max(-line.delta, 0), 0);
  return [
    ...(gain ? [
      { id: config.inventory_account_id, debit: gain, credit: 0 },
      { id: config.variance_account_id, debit: 0, credit: gain },
    ] : []),
    ...(loss ? [
      { id: config.variance_account_id, debit: loss, credit: 0 },
      { id: config.inventory_account_id, debit: 0, credit: loss },
    ] : []),
  ];
}
