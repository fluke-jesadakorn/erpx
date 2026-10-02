import "server-only";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import { access, fail } from "./access";
import { documentAccess } from "./documents";
import { lawCitation, legalEvidenceDocumentAvailableSql } from "./knowledge";

export async function legalCitation(db: SqlDatabase, actor: WorkContext, sourceId: string, asOf?: unknown) {
  if (sourceId.startsWith("law:")) return lawCitation(db, actor, sourceId, asOf);
  const id = sourceId.replace(/^document:/, "");
  const chunk = await db.prepare("SELECT c.id,c.document_id,CASE WHEN c.page_mapping THEN c.page ELSE NULL END page,c.page_mapping,c.section,c.text,c.document_checksum FROM legal_source_chunks c JOIN legal_text_reviews r ON r.id=c.text_review_id AND r.state='approved' AND r.company_id=c.company_id AND r.item_id=c.item_id AND r.document_id=c.document_id AND r.document_checksum=c.document_checksum WHERE c.id=? AND c.company_id=?").bind(id, actor.companyId).first<{ id: string; document_id: string; page: number | null; page_mapping: boolean; section: string; text: string; document_checksum: string }>();
  if (sourceId.startsWith("document:") && !chunk) fail("The reviewed passage is unavailable.", "LEGAL_CITATION_UNAVAILABLE", 404);
  const document = await documentAccess(db, actor, chunk?.document_id ?? id);
  const item = await access(db, actor, document.item_id);
  if (!(await db.prepare(`SELECT 1 FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE d.id=? AND ${legalEvidenceDocumentAvailableSql}`).bind(document.id).first())) fail("The cited original is unavailable or awaiting security review.", "LEGAL_CITATION_UNAVAILABLE", 404);
  if (chunk && chunk.document_checksum !== document.checksum) fail("The source version changed.", "LEGAL_CITATION_UNAVAILABLE", 404);
  if (!chunk && (document.media_type !== "text/plain" || document.object_key || item.kind === "rule")) fail("A reviewed source passage is required.", "LEGAL_CITATION_UNAVAILABLE", 404);
  await db.prepare("INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)").bind(crypto.randomUUID(), item.id, document.id, actor.userId, "document.citation.read", Date.now()).run();
  return { kind: "document" as const, title: item.title, name: document.name, item_id: item.id, document_id: document.id, version: document.version, checksum: document.checksum, page: chunk?.page ?? null, page_mapping: chunk?.page_mapping ?? false, section: chunk?.section ?? "", text: chunk?.text ?? document.body, notice: "Read this immutable original and verify that the cited passage supports the conclusion. Legal interpretation requires independent review." };
}
