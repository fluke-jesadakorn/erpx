import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { inventoryReadProjection as commercePostingProjection } from "@erpx/server-commerce/public/inventory-read-projections";
import { documentPostingPreviewFacts as inventoryFacts } from "@erpx/server-inventory/public/document-posting-preview-facts";
import { documentPostingPreviewFacts as procurementFacts } from "@erpx/server-procurement/public/document-posting-preview-facts";
import { createDocumentPostingPreview } from "@erpx/server-finance/application/document-posting-preview";
import type { PostingPreviewConfiguration, PostingPreviewLine, PostingPreviewRow } from "@erpx/server-finance/domain/document-posting-preview";

export async function documentPostingPreview(actor: WorkContext, kind: "invoice" | "stock_count", id: string, provided?: SqlDatabase) {
  const db = provided ?? (await holdingResources(actor.holdingId)).write;
  return createDocumentPostingPreview({
    ...inventoryFacts(db), ...procurementFacts(db),
    invoice: (companyId, invoiceId) => db.prepare("SELECT * FROM finance_invoices WHERE company_id=? AND id=?").bind(companyId, invoiceId).first<PostingPreviewRow>(),
    accounts: async companyId => (await db.prepare("SELECT id,code,name,status FROM finance_accounts WHERE company_id=?").bind(companyId).all<PostingPreviewRow>()).results,
    journalLines: async journalId => (await db.prepare("SELECT account_id id,debit_minor debit,credit_minor credit FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal").bind(journalId).all<PostingPreviewLine>()).results,
    stockConfiguration: companyId => db.prepare(`SELECT s.inventory_account_id,a.id variance_account_id ${commercePostingProjection.postingSettings} JOIN finance_accounts a ON a.chart_id=s.chart_id AND a.company_id=s.company_id AND a.code='5400' AND a.category='expense' AND a.status='active' WHERE s.company_id=?`).bind(companyId).first<PostingPreviewConfiguration>(),
  })(actor.companyId, kind, id);
}

export async function editedCustomerParty(actor: WorkContext, id: string, provided?: SqlDatabase) {
  const db = provided ?? (await holdingResources(actor.holdingId)).write;
  return db.prepare("SELECT * FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND person_id IS NULL").bind(id, actor.companyId).first<PostingPreviewRow>();
}
