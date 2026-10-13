import "server-only";
import type { SqlDatabase } from "../../../runtime/public/runtime/types";
import type { WorkContext } from "../../../identity_access/public/authorization";
import { ApiError } from "../../../runtime/public/http";
import { access } from "./access";
import { documentAccess } from "@/server/modules/legal/public/legal/documents";
import { lawCitation, legalEvidenceDocumentAvailableSql } from "./knowledge";
import { createLegalCitationQuery, type LegalDocumentCitationChunk } from "../../application/citations";
export function legalCitation(db: SqlDatabase, actor: WorkContext, sourceId: string, asOf?: unknown) {
  return createLegalCitationQuery({ law: (id, asOf) => lawCitation(db, actor, id, asOf), document: id => documentAccess(db, actor, id), item: id => access(db, actor, id), clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, error: (status, code, message) => new ApiError(status, code, message),
    chunk: id => db.prepare("SELECT c.id,c.document_id,CASE WHEN c.page_mapping THEN c.page ELSE NULL END page,c.page_mapping,c.section,c.text,c.document_checksum FROM legal_source_chunks c JOIN legal_text_reviews r ON r.id=c.text_review_id AND r.state='approved' AND r.company_id=c.company_id AND r.item_id=c.item_id AND r.document_id=c.document_id AND r.document_checksum=c.document_checksum WHERE c.id=? AND c.company_id=?").bind(id, actor.companyId).first<LegalDocumentCitationChunk>(),
    available: async id => Boolean(await db.prepare(`SELECT 1 FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE d.id=? AND ${legalEvidenceDocumentAvailableSql}`).bind(id).first()),
    recordRead: async (itemId, documentId, identifier, now) => { await db.prepare("INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)").bind(identifier, itemId, documentId, actor.userId, "document.citation.read", now).run(); },
  })(sourceId, asOf);
}
