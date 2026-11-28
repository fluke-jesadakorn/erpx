import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {LINK_SET_ATTACHMENT_EXISTS_SQL} from "./read-tail-dedupe-sql";
import {
  readTaxSignatureMetadata,
  readTaxSignatureWorkspace,
  type FilesCommercialActor,
} from "./native-lane-21/client";

/**
 * Files-owned tax-signature attachment reads. The two SELECTs now belong to
 * the native Files lane-21 axum router; this module is a strict transport. The
 * `db` argument is retained for signature compatibility and never issues a
 * fallback query. The signature guard stays here: it is a write assertion
 * executed inside the caller's command transaction, not a read.
 */
const holding = (actor: WorkContext): FilesCommercialActor => ({
  holdingId: actor.holdingId,
  holdingSlug: actor.holdingSlug,
  companyId: actor.companyId,
});

export async function taxSignatureMetadata(
  _db: SqlDatabase,
  actor: WorkContext,
  id: string,
  linkId: string,
) {
  void _db;
  return readTaxSignatureMetadata(holding(actor), id, linkId);
}

export function taxSignatureGuard(db: SqlDatabase, actor: WorkContext, linkId: string, id: string, sha256: string, commandId: string, tag: string) { return [assertion(db, commandId, tag, `${LINK_SET_ATTACHMENT_EXISTS_SQL} WHERE l.id=? AND s.company_id=? AND s.target_id=? AND s.target_type='finance_tax_identity' AND l.state='bound' AND s.state='bound' AND a.source_state='available' AND a.checksum_sha256=?)`, [linkId, actor.companyId, id, sha256])]; }

export async function taxSignatureWorkspace(_db: SqlDatabase, actor: WorkContext) {
  void _db;
  return readTaxSignatureWorkspace(holding(actor));
}
