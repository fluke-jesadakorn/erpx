import { invoicePreviewLines, inventoryPreviewLines, type DocumentPostingPreview, type PostingPreviewConfiguration, type PostingPreviewLine, type PostingPreviewRow } from "../domain/document-posting-preview";

export type DocumentPostingPreviewPorts = {
  invoice(companyId: string, id: string): Promise<PostingPreviewRow | null>;
  stockCount(companyId: string, id: string): Promise<PostingPreviewRow | null>;
  accounts(companyId: string): Promise<PostingPreviewRow[]>;
  journalLines(journalId: string | number): Promise<PostingPreviewLine[]>;
  matchedInvoiceLines(companyId: string, id: string): Promise<PostingPreviewLine[] | null>;
  stockConfiguration(companyId: string): Promise<PostingPreviewConfiguration | null>;
  stockDeltas(id: string): Promise<{ delta: number }[]>;
};

export function createDocumentPostingPreview(ports: DocumentPostingPreviewPorts) {
  return async (companyId: string, kind: "invoice" | "stock_count", id: string): Promise<DocumentPostingPreview | null> => {
    const row = await (kind === "invoice" ? ports.invoice(companyId, id) : ports.stockCount(companyId, id));
    if (!row) return null;
    if (kind === "invoice" && row.status === "voided") return { state: "voided" };
    if (kind === "invoice" && row.status === "no_charge_confirmed") return { state: "no-charge-confirmed" };
    const accounts = await ports.accounts(companyId);
    let lines: PostingPreviewLine[];
    if (row.journal_id) lines = await ports.journalLines(row.journal_id);
    else if (kind === "invoice") lines = await ports.matchedInvoiceLines(companyId, id) ?? invoicePreviewLines(row);
    else {
      const config = await ports.stockConfiguration(companyId);
      if (!config) return { state: "configuration-required" };
      lines = inventoryPreviewLines(config, await ports.stockDeltas(id));
    }
    return { state: "ready", posted: Boolean(row.journal_id), accounts, lines,
      debit: lines.reduce((sum, line) => sum + line.debit, 0), credit: lines.reduce((sum, line) => sum + line.credit, 0) };
  };
}
