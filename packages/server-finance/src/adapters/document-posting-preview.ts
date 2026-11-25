import { tableByIdSql } from './finance-read-tail-r49-sql';
import { CUSTOMER_PARTY_PROFILE_SQL } from './finance-read-tail-r38-sql';
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { documentPostingPreviewFacts as inventoryFacts } from "@erpx/server-inventory/public/document-posting-preview-facts";
import { documentPostingPreviewFacts as procurementFacts } from "@erpx/server-procurement/public/document-posting-preview-facts";
import { createDocumentPostingPreview } from "@erpx/server-finance/application/document-posting-preview";
import type { PostingPreviewConfiguration, PostingPreviewLine, PostingPreviewRow } from "@erpx/server-finance/domain/document-posting-preview";
import {
  documentPostingAccounts,
  documentPostingJournalLines,
  documentPostingStockConfiguration,
} from './native-lane-1/doc-preview-client';

/**
 * The three Finance-owned preview reads (accounts, posted journal lines and
 * stock configuration) are now executed by the native lane 1 axum router.
 * This adapter keeps the original application ports and cross-owner fact
 * inputs; it contains none of the replaced SQL and has no fallback.
 */
export async function documentPostingPreview(actor: WorkContext, kind: "invoice" | "stock_count", id: string, provided?: SqlDatabase) {
  const db = provided ?? (await holdingResources(actor.holdingId)).write;
  return createDocumentPostingPreview({
    ...inventoryFacts(db), ...procurementFacts(db),
    invoice: (companyId, invoiceId) => db.prepare(tableByIdSql('finance_invoices')).bind(invoiceId, companyId).first<PostingPreviewRow>(),
    accounts: companyId => documentPostingAccounts(db, actor, companyId),
    journalLines: journalId => documentPostingJournalLines(db, actor, journalId),
    stockConfiguration: companyId => documentPostingStockConfiguration(db, actor, companyId),
  })(actor.companyId, kind, id);
}

export async function editedCustomerParty(actor: WorkContext, id: string, provided?: SqlDatabase) {
  const db = provided ?? (await holdingResources(actor.holdingId)).write;
  return db.prepare(CUSTOMER_PARTY_PROFILE_SQL).bind(id, actor.companyId).first<PostingPreviewRow>();
}
