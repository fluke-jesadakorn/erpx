import "server-only";
import { z } from "zod";
import type { SqlDatabase } from "../runtime/types";
import type { WorkContext } from "../authorization";
import { requirePermission } from "../iam-permissions";
import { dateSchema } from "@/lib/legal/model";
import { access, fail, visibility } from "./access";
import { documentAccess } from "./documents";

export type LegalEvidenceSource = {
  id: string;
  sourceId: string;
  kind: "law" | "document";
  item_id: string;
  title: string;
  document_id: string;
  version: number;
  checksum: string;
  text: string;
  excerpt: string;
  page: number | null;
  page_mapping?: boolean;
  section: string;
  revision_id?: string;
  revision_label?: string;
  source_url?: string;
  published_on?: string;
  effective_from?: string;
  effective_until?: string | null;
  text_review_id?: string;
  name?: string;
};
/** Shared by search and direct citations; aliases must be legal_documents d and legal_items i. */
export const legalEvidenceDocumentAvailableSql = `(d.object_key IS NULL OR EXISTS(SELECT 1 FROM attachment_links clean_link JOIN attachments clean_file ON clean_file.id=clean_link.attachment_id WHERE clean_link.id=d.attachment_link_id AND clean_link.state='bound' AND clean_file.company_id=i.company_id AND clean_file.source_state='available' AND clean_file.revoked_at IS NULL AND clean_file.checksum_sha256=d.checksum)) AND NOT EXISTS(SELECT 1 FROM legal_disposals disposed WHERE disposed.document_id=d.id) AND NOT EXISTS(SELECT 1 FROM legal_destructions destruction WHERE destruction.item_id=i.id AND destruction.state IN ('executing','completed'))`;
const cleanDocument = legalEvidenceDocumentAvailableSql;
const lawAvailability = `r.state='approved' AND r.effective_from<=? AND (r.effective_until IS NULL OR r.effective_until>?) AND s.state='active' AND i.state NOT IN ('archived','cancelled','rejected') AND ${cleanDocument}
  AND (SELECT ap.applies FROM legal_law_applicability ap WHERE ap.company_id=i.company_id AND ap.revision_id=r.id AND ap.state='approved' ORDER BY ap.created_at DESC,ap.id DESC LIMIT 1)=true`;
const unambiguousRevision = `(SELECT count(*) FROM legal_law_revisions conflicting WHERE conflicting.source_id=r.source_id AND conflicting.state='approved' AND conflicting.effective_from<=? AND (conflicting.effective_until IS NULL OR conflicting.effective_until>?))=1`;
function asOfDate(raw?: unknown) {
  return raw == null ? new Date(Date.now() + 7 * 3600000).toISOString().slice(0, 10) : dateSchema.parse(raw);
}
async function recordReads(db: SqlDatabase, a: WorkContext, sources: LegalEvidenceSource[], action: string) {
  const unique = Array.from(new Map(sources.map((source) => [source.document_id, source])).values());
  if (unique.length) await db.batch(unique.map((source) => db.prepare("INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)").bind(crypto.randomUUID(), source.item_id, source.document_id, a.userId, action, Date.now())));
}
const lawFields = `c.id,'law:'||c.id "sourceId",'law' kind,i.id item_id,i.title,d.id document_id,d.version,d.checksum,c.text,c.text excerpt,CASE WHEN c.page_mapping THEN c.page ELSE NULL END page,c.page_mapping,c.section,r.id revision_id,r.revision_label,s.source_url,r.published_on,r.effective_from,r.effective_until`;

export async function lawKnowledge(db: SqlDatabase, a: WorkContext, query: unknown, rawAsOf?: unknown) {
  requirePermission(a, "legal.read");
  const q = z.string().trim().min(2).max(200).parse(query), asOf = asOfDate(rawAsOf), v = visibility(a);
  const sources = (await db.prepare(`SELECT ${lawFields},ts_rank_cd(c.search,websearch_to_tsquery('simple',?))+word_similarity(?,c.text) rank FROM legal_source_chunks c JOIN legal_law_revisions r ON r.id=c.revision_id JOIN legal_law_sources s ON s.id=r.source_id JOIN legal_documents d ON d.id=c.document_id JOIN legal_items i ON i.id=c.item_id WHERE ${v.sql} AND ${lawAvailability} AND ${unambiguousRevision} AND (c.search@@websearch_to_tsquery('simple',?) OR c.text %> ? OR strpos(lower(c.text),lower(?))>0) ORDER BY rank DESC,r.effective_from DESC,c.id LIMIT 30`).bind(q, q, ...v.values, asOf, asOf, asOf, asOf, q, q, q).all<LegalEvidenceSource>()).results;
  const conflicts = (await db.prepare(`SELECT count(DISTINCT r.source_id) n FROM legal_law_revisions r JOIN legal_law_sources s ON s.id=r.source_id JOIN legal_documents d ON d.id=r.document_id JOIN legal_items i ON i.id=r.rule_id WHERE ${v.sql} AND ${lawAvailability} AND NOT ${unambiguousRevision}`).bind(...v.values, asOf, asOf, asOf, asOf).first<{ n: number }>())?.n ?? 0;
  await recordReads(db, a, sources, "law.knowledge.read");
  return { query: q, asOf, sources, conflicts: Number(conflicts), notice: Number(conflicts) ? "Overlapping approved law revisions need independent review. Conflicting sources are excluded; no legal conclusion can be inferred for them." : sources.length ? "Only independently approved text and Company applicability effective on the selected Gregorian date are shown. Verify that each cited provision supports the conclusion." : "No approved applicable official-law source matched this date. Evidence is insufficient for a legal conclusion; request the official original and an independent applicability review." };
}
export async function lawCitation(db: SqlDatabase, a: WorkContext, chunkId: string, rawAsOf?: unknown) {
  requirePermission(a, "legal.read");
  const asOf = asOfDate(rawAsOf), v = visibility(a), id = z.string().min(1).max(200).parse(chunkId).replace(/^law:/, "");
  const source = await db.prepare(`SELECT ${lawFields} FROM legal_source_chunks c JOIN legal_law_revisions r ON r.id=c.revision_id JOIN legal_law_sources s ON s.id=r.source_id JOIN legal_documents d ON d.id=c.document_id JOIN legal_items i ON i.id=c.item_id WHERE c.id=? AND ${v.sql} AND ${lawAvailability} AND ${unambiguousRevision}`).bind(id, ...v.values, asOf, asOf, asOf, asOf).first<LegalEvidenceSource>();
  if (!source) fail("The cited law source is unavailable, unapproved, conflicting or not applicable on this date.", "LEGAL_CITATION_UNAVAILABLE", 404);
  await recordReads(db, a, [source], "law.citation.read");
  return { ...source, asOf, href: `/legal?record=${encodeURIComponent(source.item_id)}`, originalUrl: `${source.source_url}${source.page && source.source_url?.endsWith(".pdf") ? `#page=${source.page}` : ""}`, notice: "The quotation and original version are retained for review. Legal interpretation still requires an independent legal reviewer." };
}
async function documentKnowledge(db: SqlDatabase, a: WorkContext, q: string) {
  const v = visibility(a);
  return (await db.prepare(`SELECT c.id,'document:'||c.id "sourceId",'document' kind,i.id item_id,i.title,d.id document_id,d.name,d.version,d.checksum,c.text,c.text excerpt,CASE WHEN c.page_mapping THEN c.page ELSE NULL END page,c.page_mapping,c.section,t.id text_review_id,ts_rank_cd(c.search,websearch_to_tsquery('simple',?))+word_similarity(?,c.text) rank FROM legal_source_chunks c JOIN legal_text_reviews t ON t.id=c.text_review_id AND t.state='approved' JOIN legal_documents d ON d.id=c.document_id AND d.checksum=c.document_checksum JOIN legal_items i ON i.id=c.item_id WHERE ${v.sql} AND i.kind<>'rule' AND ${cleanDocument} AND (c.search@@websearch_to_tsquery('simple',?) OR c.text %> ? OR strpos(lower(c.text),lower(?))>0) ORDER BY rank DESC,d.created_at DESC,c.id LIMIT 30`).bind(q, q, ...v.values, q, q, q).all<LegalEvidenceSource>()).results;
}
export async function legalKnowledge(db: SqlDatabase, a: WorkContext, query: unknown, rawAsOf?: unknown) {
  requirePermission(a, "legal.read");
  const q = z.string().trim().min(2).max(200).parse(query), v = visibility(a);
  const reviewed = await documentKnowledge(db, a, q);
  // Authored immutable plain text has no OCR page claim. File extracts require the independent review above.
  const authored = (await db.prepare(`SELECT d.id,d.id "sourceId",'document' kind,i.id item_id,i.title,d.id document_id,d.name,d.version,d.checksum,d.body text,substring(d.body from greatest(1,strpos(lower(d.body),lower(?))-120) for 500) excerpt,NULL::int page,'' section FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE ${v.sql} AND i.kind<>'rule' AND d.object_key IS NULL AND d.media_type='text/plain' AND ${cleanDocument} AND strpos(lower(d.body),lower(?))>0 ORDER BY i.updated_at DESC,d.version DESC LIMIT 20`).bind(q, ...v.values, q).all<LegalEvidenceSource>()).results;
  const laws = await lawKnowledge(db, a, q, rawAsOf);
  const documents = [...reviewed, ...authored];
  await recordReads(db, a, documents, "knowledge.read");
  const sources = [...documents, ...laws.sources];
  return { query: q, asOf: laws.asOf, sources, conflicts: laws.conflicts, notice: sources.length ? laws.conflicts ? laws.notice : "Review each cited immutable version and exact page or section before relying on its text. Extracted file text and official laws require independent review." : "No accessible reviewed source matched. Evidence is insufficient; request the missing documents or independently approved official law revision." };
}
export async function retrieveLegalAiEvidence(db: SqlDatabase, a: WorkContext, itemId: string, question: string) {
  await access(db, a, itemId, "legal.ai");
  const result = await legalKnowledge(db, a, question.slice(0, 200));
  const v = visibility(a);
  const authored = (await db.prepare(`SELECT d.id,d.id "sourceId",'document' kind,i.id item_id,i.title,d.id document_id,d.name,d.version,d.checksum,d.body text,d.body excerpt,NULL::int page,'' section FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE i.id=? AND ${v.sql} AND i.kind<>'rule' AND d.object_key IS NULL AND d.media_type='text/plain' AND length(d.body)>0 AND ${cleanDocument} ORDER BY d.version DESC LIMIT 5`).bind(itemId, ...v.values).all<LegalEvidenceSource>()).results;
  const sources = Array.from(new Map([...result.sources, ...authored].map((source) => [source.sourceId, { ...source, text: source.text.slice(0, 12000), excerpt: source.excerpt.slice(0, 12000) }])).values()).slice(0, 20);
  await recordReads(db, a, authored, "ai.source.read");
  return { sources, asOf: result.asOf, conflicts: result.conflicts, insufficientEvidence: sources.length === 0 || result.conflicts > 0, notice: result.notice };
}
export async function revalidateLegalAiEvidence(db: SqlDatabase, a: WorkContext, evidence: Awaited<ReturnType<typeof retrieveLegalAiEvidence>>) {
  for (const source of evidence.sources) {
    if (source.kind === "law") { const fresh = await lawCitation(db, a, source.id, evidence.asOf); if (fresh.checksum !== source.checksum || fresh.revision_id !== source.revision_id) fail("The law source changed during generation.", "LEGAL_CITATION_UNAVAILABLE", 409); }
    else {
      const d = await documentAccess(db, a, source.document_id);
      if (d.checksum !== source.checksum) fail("A cited source changed.", "LEGAL_CITATION_UNAVAILABLE", 409);
      const v = visibility(a);
      if (!(await db.prepare(`SELECT 1 FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE d.id=? AND ${v.sql} AND ${cleanDocument}`).bind(d.id, ...v.values).first())) fail("A cited source is no longer available.", "LEGAL_CITATION_UNAVAILABLE", 409);
      if (source.text_review_id && !(await db.prepare("SELECT 1 FROM legal_text_reviews WHERE id=? AND document_id=? AND state='approved'").bind(source.text_review_id, d.id).first())) fail("Extracted text is no longer approved.", "LEGAL_CITATION_UNAVAILABLE", 409);
    }
  }
}
type CitationClaim = { text: string; sources: string[]; quotes?: { sourceId: string; quote: string }[] };
/** Verify references and exact evidence quotation, without claiming automated legal entailment. */
export function validateLegalAiCitations<T extends { sources: string[]; claims: CitationClaim[] }>(proposal: T, evidence: Awaited<ReturnType<typeof retrieveLegalAiEvidence>>) {
  const sources = new Map(evidence.sources.map((source) => [source.sourceId, source]));
  if ([...proposal.sources, ...proposal.claims.flatMap((claim) => claim.sources)].some((id) => !sources.has(id))) fail("AI returned an inaccessible or unsupported source reference.", "LEGAL_AI_CITATION", 422);
  const claims = proposal.claims.map((claim) => {
    if (!claim.sources.length) return { ...claim, supportStatus: "insufficient_evidence", requiresHumanReview: true };
    for (const sourceId of claim.sources) {
      const quote = claim.quotes?.find((entry) => entry.sourceId === sourceId)?.quote;
      if (!quote || quote.trim().length < 8 || !sources.get(sourceId)!.text.normalize("NFC").includes(quote.normalize("NFC"))) fail("Each supported AI claim must quote the exact supplied evidence. Citation presence alone does not establish support.", "LEGAL_AI_CITATION", 422);
    }
    return { ...claim, supportStatus: "quotation_verified_pending_legal_review", requiresHumanReview: true };
  });
  return { ...proposal, claims, evidenceStatus: evidence.insufficientEvidence || !claims.length || claims.some((claim) => claim.supportStatus === "insufficient_evidence") ? "insufficient_evidence" : "pending_legal_review", limitations: ["The server verified source access, revision, date and exact quotations. An independent reviewer must determine whether each quotation supports its legal conclusion.", ...(!claims.length ? ["No supported claims were supplied. A source list alone cannot support the summary or draft."] : []), ...(evidence.insufficientEvidence ? [evidence.notice] : [])], citations: evidence.sources.filter((source) => proposal.sources.includes(source.sourceId) || proposal.claims.some((claim) => claim.sources.includes(source.sourceId))).map(({ text: _text, excerpt: _excerpt, ...source }) => { void _text; void _excerpt; return { ...source, asOf: evidence.asOf }; }) };
}
