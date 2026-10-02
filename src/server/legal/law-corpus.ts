import "server-only";
import { z } from "zod";
import { lookup } from "node:dns/promises";
import { request as httpsRequest } from "node:https";
import type { WorkContext } from "../authorization";
import type { SqlDatabase, FileStore, SqlStatement } from "../runtime/types";
import { requirePermission } from "../iam-permissions";
import { assertion, executeCommand, replayCommand } from "../commands";
import { samePerson } from "@/lib/iam";
import { dateSchema } from "@/lib/legal/model";
import { publicAddress } from "../ai-transport";
import { access, guards, event, fail, visibility } from "./access";
import { documentAccess } from "./documents";
import { hash } from "./service";
import { legalCompanyCapabilityReady, legalRuntimePredicate } from "./runtime-gate";

export const legalChunkSchema = z.object({
  page: z.number().int().min(1).max(10000),
  pageMapping: z.boolean().default(true),
  section: z.string().trim().max(200).default(""),
  text: z.string().trim().min(1).max(12000),
});
const chunksSchema = z.array(legalChunkSchema).min(1).max(300);
type Chunk = z.infer<typeof legalChunkSchema>;
type ReviewRow = { id: string; version: number; state: string; created_by: string; document_id: string; document_checksum: string };

export function officialLawUrl(value: string) {
  let url: URL;
  try { url = new URL(value); } catch { fail("Use the official source's HTTPS URL.", "LEGAL_SOURCE_URL"); }
  if (url.protocol !== "https:" || url.username || url.password || url.port || url.hash || !url.hostname.endsWith(".go.th") || url.hostname.endsWith("."))
    fail("Use an official Thai government HTTPS source without credentials, a port or a fragment.", "LEGAL_SOURCE_URL");
  return url.href;
}
export function bangkokSourceCheckDate(now = Date.now()) {
  const local = new Date(now + 7 * 3600000);
  return local.getUTCHours() < 6 ? null : local.toISOString().slice(0, 10);
}
async function readableSourceDocument(db: SqlDatabase, a: WorkContext, id: string, itemId: string) {
  const d = await documentAccess(db, a, id);
  if (d.item_id !== itemId) fail("Choose an immutable document from this record.");
  if (d.object_key && !(await db.prepare("SELECT 1 FROM attachment_links l JOIN attachments f ON f.id=l.attachment_id WHERE l.id=? AND l.state='bound' AND f.company_id=? AND f.source_state='available' AND f.revoked_at IS NULL AND f.checksum_sha256=?").bind(d.attachment_link_id, a.companyId, d.checksum).first()))
    fail("The original must pass quarantine and scanning before its text can be reviewed.", "LEGAL_SOURCE_QUARANTINED", 409);
  return d;
}
function sourceDocumentGuard(db: SqlDatabase, a: WorkContext, key: string, documentId: string, itemId: string, checksum: string) {
  return assertion(db, key, "legal-corpus-source-clean", `EXISTS(SELECT 1 FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE d.id=? AND d.item_id=? AND i.company_id=? AND d.checksum=? AND NOT EXISTS(SELECT 1 FROM legal_disposals x WHERE x.document_id=d.id) AND (d.object_key IS NULL OR EXISTS(SELECT 1 FROM attachment_links l JOIN attachments f ON f.id=l.attachment_id WHERE l.id=d.attachment_link_id AND l.state='bound' AND f.company_id=i.company_id AND f.source_state='available' AND f.revoked_at IS NULL AND f.checksum_sha256=d.checksum)))`, [documentId, itemId, a.companyId, checksum]);
}
async function extractionEvidence(db: SqlDatabase, documentId: string, checksum: string, method: string, inspectionId?: string) {
  if (method === "ocr" && !inspectionId) fail("Select the extraction inspection for this immutable original.");
  if (!inspectionId) return null;
  const row = await db.prepare("SELECT result_json FROM legal_document_inspections WHERE id=? AND document_id=? AND kind='ocr'").bind(inspectionId, documentId).first<{ result_json: string }>();
  const result = row ? (typeof row.result_json === "string" ? JSON.parse(row.result_json) : row.result_json) : null;
  if (!result || result.checksum !== checksum) fail("The extraction does not match this immutable original.", "LEGAL_SOURCE_CHECKSUM", 409);
  return result as { pageMapping?: boolean };
}
function insertChunks(db: SqlDatabase, a: WorkContext, itemId: string, documentId: string, checksum: string, chunks: Chunk[], revisionId: string | null, reviewId: string | null): SqlStatement[] {
  return chunks.map((chunk) => db.prepare("INSERT INTO legal_source_chunks(id,company_id,item_id,document_id,document_checksum,revision_id,text_review_id,page,page_mapping,section,text,text_checksum) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(), a.companyId, itemId, documentId, checksum, revisionId, reviewId, chunk.page, chunk.pageMapping, chunk.section, chunk.text, hash(chunk.text)));
}
export async function lawCorpusCommand(db: SqlDatabase, a: WorkContext, raw: unknown, key: string) {
  const x = z.object({ action: z.enum(["law.source.register", "law.revision.import", "law.revision.approve", "law.revision.retire", "law.applicability.propose", "law.applicability.approve", "knowledge.document.import", "knowledge.document.approve"]), id: z.string().min(1), expectedVersion: z.number().int().positive(), reason: z.string().trim().min(3).max(4000) }).parse(raw);
  const approving = x.action.endsWith("approve") || x.action.endsWith("retire");
  requirePermission(a, x.action === "law.source.register" ? "legal.manage" : approving ? "legal.approve" : "legal.edit");
  if (x.action.startsWith("law.") && (a.branchId || a.departmentId)) fail("Manage the law corpus through a Company assignment.", "LEGAL_COMPANY_SCOPE", 403);
  const item = await access(db, a, x.id, approving ? "legal.read" : "legal.edit");
  if (x.action.startsWith("law.") && item.kind !== "rule") fail("Attach official law evidence to a jurisdiction rule.");
  if (x.action.startsWith("knowledge.") && item.kind === "rule") fail("Official law text requires a law revision and a separate Company applicability review.");
  const prior = await replayCommand(db, a, key, x.action, item.id, raw);
  if (prior) return prior;
  if (item.version !== x.expectedVersion) fail("The record changed. Refresh first.", "LEGAL_VERSION_CONFLICT", 409);
  const steps = guards(db, a, item.id, x.expectedVersion, key, !approving, false, x.action.startsWith("law."));
  let evidenceId = key;
  if (x.action === "law.source.register") {
    const p = z.object({ sourceUrl: z.string().max(2000), title: z.string().trim().min(3).max(300) }).parse(raw);
    steps.push(db.prepare("INSERT INTO legal_law_sources(id,company_id,rule_id,title,source_url,created_by,created_at) VALUES(?,?,?,?,?,?,?)").bind(key, a.companyId, item.id, p.title, officialLawUrl(p.sourceUrl), a.userId, Date.now()));
  } else if (x.action === "law.revision.import" || x.action === "knowledge.document.import") {
    const p = z.object({ documentId: z.string(), extractionMethod: z.enum(["manual", "ocr"]), inspectionId: z.string().optional(), chunks: chunksSchema }).parse(raw);
    if (p.chunks.reduce((total, chunk) => total + chunk.text.length, 0) > 200000) fail("Reviewed text is limited to 200,000 characters per import.");
    const d = await readableSourceDocument(db, a, p.documentId, item.id);
    steps.push(sourceDocumentGuard(db, a, key, d.id, item.id, d.checksum));
    const extraction = await extractionEvidence(db, d.id, d.checksum, p.extractionMethod, p.inspectionId);
    if ((p.extractionMethod === "ocr" && extraction?.pageMapping !== true) || d.media_type === "application/vnd.openxmlformats-officedocument.wordprocessingml.document") p.chunks = p.chunks.map((chunk) => ({ ...chunk, pageMapping: false }));
    if (x.action === "law.revision.import") {
      const r = z.object({ sourceId: z.string(), sourceCheckId: z.string().optional(), revisionLabel: z.string().trim().min(1).max(200), publishedOn: dateSchema, effectiveFrom: dateSchema, effectiveUntil: dateSchema.optional() }).parse(raw);
      if (r.effectiveUntil && r.effectiveUntil <= r.effectiveFrom) fail("Effective date range is reversed.");
      if (!(await db.prepare("SELECT 1 FROM legal_law_sources WHERE id=? AND rule_id=? AND company_id=? AND state='active'").bind(r.sourceId, item.id, a.companyId).first())) fail("Choose this rule's registered official source.");
      if (r.sourceCheckId && !(await db.prepare("SELECT 1 FROM legal_law_source_checks WHERE id=? AND source_id=? AND company_id=? AND state='pending_review' AND checksum=? AND candidate_revision_id IS NULL").bind(r.sourceCheckId, r.sourceId, a.companyId, d.checksum).first())) fail("The uploaded original must match the pending official-source download checksum.", "LEGAL_SOURCE_CHECKSUM", 409);
      steps.push(assertion(db, key, "law-source-current", "EXISTS(SELECT 1 FROM legal_law_sources WHERE id=? AND company_id=? AND rule_id=? AND state='active')", [r.sourceId, a.companyId, item.id]), db.prepare("INSERT INTO legal_law_revisions(id,company_id,rule_id,source_id,document_id,document_checksum,revision_label,published_on,effective_from,effective_until,extraction_method,inspection_id,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(key, a.companyId, item.id, r.sourceId, d.id, d.checksum, r.revisionLabel, r.publishedOn, r.effectiveFrom, r.effectiveUntil ?? null, p.extractionMethod, p.inspectionId ?? null, a.userId, Date.now()), ...insertChunks(db, a, item.id, d.id, d.checksum, p.chunks, key, null));
      if (r.sourceCheckId) steps.push(assertion(db, key, "law-source-check-current", "EXISTS(SELECT 1 FROM legal_law_source_checks WHERE id=? AND state='pending_review' AND checksum=? AND candidate_revision_id IS NULL)", [r.sourceCheckId, d.checksum]), db.prepare("UPDATE legal_law_source_checks SET candidate_revision_id=? WHERE id=?").bind(key, r.sourceCheckId));
    } else steps.push(db.prepare("INSERT INTO legal_text_reviews(id,company_id,item_id,document_id,document_checksum,extraction_method,inspection_id,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(key, a.companyId, item.id, d.id, d.checksum, p.extractionMethod, p.inspectionId ?? null, a.userId, Date.now()), ...insertChunks(db, a, item.id, d.id, d.checksum, p.chunks, null, key));
  } else if (x.action === "law.applicability.propose") {
    const p = z.object({ revisionId: z.string(), applies: z.boolean(), basis: z.string().trim().min(10).max(12000) }).parse(raw);
    if (!(await db.prepare("SELECT 1 FROM legal_law_revisions WHERE id=? AND rule_id=? AND company_id=? AND state='approved'").bind(p.revisionId, item.id, a.companyId).first())) fail("Assess an independently approved law revision.");
    steps.push(assertion(db, key, "law-revision-current", "EXISTS(SELECT 1 FROM legal_law_revisions WHERE id=? AND rule_id=? AND state='approved')", [p.revisionId, item.id]), db.prepare("INSERT INTO legal_law_applicability(id,company_id,rule_id,revision_id,applies,basis,created_by,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(key, a.companyId, item.id, p.revisionId, p.applies, p.basis, a.userId, Date.now()));
  } else if (x.action === "law.applicability.approve") {
    const p = z.object({ applicabilityId: z.string(), applicabilityVersion: z.number().int().positive() }).parse(raw);
    const row = await db.prepare("SELECT id,version,state,created_by FROM legal_law_applicability WHERE id=? AND rule_id=? AND company_id=?").bind(p.applicabilityId, item.id, a.companyId).first<ReviewRow>();
    if (!row || row.version !== p.applicabilityVersion || row.state !== "pending") fail("Applicability changed. Refresh.", "LEGAL_VERSION_CONFLICT", 409);
    if (samePerson(a, row.created_by)) fail("An independent reviewer must verify Company applicability.", "LEGAL_INDEPENDENT_REVIEW", 403);
    evidenceId = row.id;
    steps.push(assertion(db, key, "law-applicability-current", "EXISTS(SELECT 1 FROM legal_law_applicability WHERE id=? AND version=? AND state='pending' AND NOT iam_same_person(created_by,?,company_id))", [row.id, row.version, a.userId]), db.prepare("UPDATE legal_law_applicability SET state='approved',reviewed_by=?,reviewed_at=?,version=version+1 WHERE id=?").bind(a.userId, Date.now(), row.id));
  } else {
    const law = x.action.startsWith("law."), retiring = x.action === "law.revision.retire";
    const p = law ? z.object({ revisionId: z.string(), revisionVersion: z.number().int().positive() }).parse(raw) : z.object({ textReviewId: z.string(), textReviewVersion: z.number().int().positive() }).parse(raw);
    const id = "revisionId" in p ? p.revisionId : p.textReviewId, version = "revisionVersion" in p ? p.revisionVersion : p.textReviewVersion;
    const table = law ? "legal_law_revisions" : "legal_text_reviews", itemColumn = law ? "rule_id" : "item_id";
    const row = await db.prepare(`SELECT * FROM ${table} WHERE id=? AND ${itemColumn}=? AND company_id=?`).bind(id, item.id, a.companyId).first<ReviewRow>();
    if (!row || row.version !== version || row.state !== (retiring ? "approved" : "pending")) fail("The source review changed. Refresh.", "LEGAL_VERSION_CONFLICT", 409);
    if (!retiring) {
      if (law) z.object({ originalVerified: z.literal(true), datesVerified: z.literal(true), textVerified: z.literal(true) }).parse(raw);
      else z.object({ textVerified: z.literal(true) }).parse(raw);
      if (samePerson(a, row.created_by)) fail("An independent reviewer must verify the original, extracted text and dates.", "LEGAL_INDEPENDENT_REVIEW", 403);
      const d = await readableSourceDocument(db, a, row.document_id, item.id);
      if (d.checksum !== row.document_checksum) fail("The source checksum changed.", "LEGAL_SOURCE_CHECKSUM", 409);
      steps.push(sourceDocumentGuard(db, a, key, d.id, item.id, d.checksum));
    }
    evidenceId = row.id;
    steps.push(assertion(db, key, "law-review-current", `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND version=? AND state=? ${retiring ? "" : "AND NOT iam_same_person(created_by,?,company_id)"})`, [row.id, row.version, row.state, ...(!retiring ? [a.userId] : [])]), db.prepare(`UPDATE ${table} SET state=?,reviewed_by=?,reviewed_at=?,review_reason=?,version=version+1 WHERE id=?`).bind(retiring ? "retired" : "approved", a.userId, Date.now(), x.reason, row.id));
    if (law && !retiring) steps.push(db.prepare("UPDATE legal_law_source_checks SET state='reviewed' WHERE candidate_revision_id=? AND state='pending_review'").bind(row.id));
  }
  steps.push(db.prepare("UPDATE legal_items SET version=version+1,updated_at=? WHERE id=?").bind(Date.now(), item.id), event(db, a, item.id, x.action, x.reason, item.version + 1));
  return executeCommand(db, a, key, x.action, raw, item.id, item.version + 1, { id: item.id, evidenceId, version: item.version + 1, state: approving ? x.action.endsWith("retire") ? "retired" : "approved" : "pending" }, steps);
}

export async function lawCorpusView(db: SqlDatabase, a: WorkContext, ruleId?: string) {
  requirePermission(a, "legal.read");
  if (ruleId) await access(db, a, ruleId);
  const v = visibility(a), clause = `${v.sql}${ruleId ? " AND i.id=?" : ""}`, args = [...v.values, ...(ruleId ? [ruleId] : [])];
  const select = async (table: string, itemColumn: string, fields = "c.*") => (await db.prepare(`SELECT ${fields},i.title record_title FROM ${table} c JOIN legal_items i ON i.id=c.${itemColumn} WHERE ${clause} ORDER BY c.created_at DESC,c.id DESC LIMIT 200`).bind(...args).all()).results;
  const ruleVisibility = visibility(a, "rule");
  const impacts = (await db.prepare(`SELECT c.id check_id,i.id item_id,i.title,i.state,i.due_date,i.version FROM legal_law_source_checks c JOIN legal_items rule ON rule.id=c.rule_id JOIN legal_links l ON l.source_id=rule.id JOIN legal_items i ON i.id=l.target_id WHERE c.state='pending_review' AND ${ruleVisibility.sql} AND ${v.sql}${ruleId ? " AND rule.id=?" : ""} ORDER BY c.created_at DESC,i.id LIMIT 200`).bind(...ruleVisibility.values, ...args).all()).results;
  const chunks = (await db.prepare(`SELECT c.*,i.title record_title FROM legal_source_chunks c JOIN legal_documents d ON d.id=c.document_id JOIN legal_items i ON i.id=c.item_id WHERE ${clause} AND NOT EXISTS(SELECT 1 FROM legal_disposals x WHERE x.document_id=c.document_id) AND NOT EXISTS(SELECT 1 FROM legal_destructions x WHERE x.item_id=i.id AND x.state IN ('executing','completed')) AND (d.object_key IS NULL OR EXISTS(SELECT 1 FROM attachment_links l JOIN attachments f ON f.id=l.attachment_id WHERE l.id=d.attachment_link_id AND l.state='bound' AND f.company_id=i.company_id AND f.source_state='available' AND f.revoked_at IS NULL AND f.checksum_sha256=d.checksum)) ORDER BY c.document_id,c.page,c.id LIMIT 1000`).bind(...args).all<{ item_id: string; document_id: string } & Record<string, unknown>>()).results;
  const documents = Array.from(new Map(chunks.map((chunk) => [chunk.document_id, chunk])).values());
  if (documents.length) await db.batch(documents.map((chunk) => db.prepare("INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)").bind(crypto.randomUUID(), chunk.item_id, chunk.document_id, a.userId, "corpus.review.read", Date.now())));
  return { sources: await select("legal_law_sources", "rule_id"), revisions: await select("legal_law_revisions", "rule_id"), applicability: await select("legal_law_applicability", "rule_id"), textReviews: await select("legal_text_reviews", "item_id"), checks: await select("legal_law_source_checks", "rule_id", "c.id,c.source_id,c.company_id,c.rule_id,c.check_date,c.state,c.attempts,c.checked_at,c.checksum,c.error,c.candidate_revision_id,c.created_at"), chunks, impacts, notice: "Official-source changes and extracted text remain pending until independently reviewed. Existing deadlines require an explicit owner decision." };
}

/** Pin public DNS results; never follow redirects or read more than the document limit. */
export async function fetchOfficialLawSource(url: string): Promise<Response> {
  const canonical = officialLawUrl(url), addresses = await lookup(new URL(canonical).hostname, { all: true });
  if (!addresses.length || addresses.some(({ address }) => !publicAddress(address))) fail("Official source DNS did not resolve exclusively to public addresses.", "LEGAL_SOURCE_DNS", 503);
  return new Promise((resolve, reject) => {
    const request = httpsRequest(canonical, { method: "GET", agent: false, signal: AbortSignal.timeout(30000), headers: { "User-Agent": "ERPX-Legal-Source-Check/1.0", Accept: "application/pdf,text/html,text/plain" }, lookup: (_host, options, callback) => options.all ? callback(null, addresses) : callback(null, addresses[0].address, addresses[0].family) }, (response) => {
      if ((response.statusCode ?? 500) < 200 || (response.statusCode ?? 500) >= 300) { response.resume(); resolve(new Response(null, { status: response.statusCode ?? 500 })); return; }
      const chunks: Uint8Array[] = []; let size = 0;
      response.on("data", (chunk: Uint8Array) => { size += chunk.byteLength; if (size > 10 * 1024 * 1024) request.destroy(new Error("LEGAL_SOURCE_TOO_LARGE")); else chunks.push(chunk); });
      response.on("error", reject);
      response.on("end", () => resolve(new Response(Buffer.concat(chunks), { status: response.statusCode, headers: { "Content-Type": String(response.headers["content-type"] ?? "application/octet-stream") } })));
    });
    request.on("error", reject); request.end();
  });
}
export async function drainLegalSourceChecks(db: SqlDatabase, files: FileStore, fetcher = fetchOfficialLawSource, now = Date.now()) {
  const date = bangkokSourceCheckDate(now);
  if (!date) return;
  await db.prepare(`INSERT INTO legal_law_source_checks(id,source_id,company_id,rule_id,check_date,created_at) SELECT 'law-check-'||md5(s.id||?),s.id,s.company_id,s.rule_id,?,? FROM legal_law_sources s WHERE s.state='active' AND ${legalRuntimePredicate("s.company_id", "sources")} ON CONFLICT(source_id,check_date) DO NOTHING`).bind(date, date, now).run();
  await db.prepare("UPDATE legal_law_source_checks SET state=CASE WHEN attempts<3 THEN 'queued' ELSE 'failed' END,error='LEGAL_SOURCE_CHECK_INTERRUPTED',lease_token=NULL,lease_until=NULL WHERE state='running' AND lease_until<?").bind(now).run();
  const token = crypto.randomUUID();
  const job = await db.prepare(`UPDATE legal_law_source_checks SET state='running',attempts=attempts+1,lease_token=?,lease_until=? WHERE id=(SELECT c.id FROM legal_law_source_checks c JOIN legal_law_sources s ON s.id=c.source_id WHERE c.state='queued' AND c.attempts<3 AND s.state='active' AND ${legalRuntimePredicate("c.company_id", "sources")} ORDER BY c.created_at FOR UPDATE OF c SKIP LOCKED LIMIT 1) RETURNING *`).bind(token, now + 120000).first<{ id: string; source_id: string; company_id: string; rule_id: string }>();
  if (!job) return;
  try {
    if (!(await legalCompanyCapabilityReady(db, job.company_id, "sources"))) fail("Source checks are disabled for this Company.", "LEGAL_SOURCE_DISABLED", 409);
    const source = await db.prepare("SELECT source_url FROM legal_law_sources WHERE id=? AND company_id=? AND state='active'").bind(job.source_id, job.company_id).first<{ source_url: string }>();
    if (!source) fail("The official source registration changed.");
    const response = await fetcher(officialLawUrl(source.source_url));
    if (!response.ok) fail("Official source unavailable; use the manual official-original import path.", `LEGAL_SOURCE_HTTP_${response.status}`, 503);
    const bytes = new Uint8Array(await response.arrayBuffer());
    if (!bytes.length || bytes.length > 10 * 1024 * 1024) fail("Official source is empty or larger than 10 MB.", "LEGAL_SOURCE_TOO_LARGE", 413);
    const checksum = hash(bytes), unchanged = !!(await db.prepare("SELECT 1 FROM legal_law_source_checks WHERE source_id=? AND id<>? AND checksum=? AND state IN ('unchanged','pending_review','reviewed') LIMIT 1").bind(job.source_id, job.id, checksum).first());
    const objectKey = `legal/${job.company_id}/${job.rule_id}/law-source/${job.id}/${checksum}`;
    await files.put(objectKey, bytes, { onlyIf: { etagDoesNotMatch: "*" }, httpMetadata: { contentType: response.headers.get("content-type") ?? "application/octet-stream" }, customMetadata: { companyId: job.company_id, legalItem: job.rule_id, checksum, quarantine: "pending" } });
    if ((await files.head(objectKey))?.etag !== checksum) fail("Official source storage checksum mismatch.", "LEGAL_SOURCE_CHECKSUM", 503);
    if (!(await legalCompanyCapabilityReady(db, job.company_id, "sources"))) fail("Source checks were disabled during download.", "LEGAL_SOURCE_DISABLED", 409);
    await db.batch([db.prepare("INSERT INTO legal_file_preservations(item_id,source_type,source_id,object_key,checksum,created_by,created_at) VALUES(?,'official-source',?,?,?,'system:legal-source',?) ON CONFLICT DO NOTHING").bind(job.rule_id, job.id, objectKey, checksum, now), db.prepare("UPDATE legal_law_source_checks SET state=?,checked_at=?,checksum=?,object_key=?,error=NULL,lease_token=NULL,lease_until=NULL WHERE id=? AND state='running' AND lease_token=?").bind(unchanged ? "unchanged" : "pending_review", now, checksum, objectKey, job.id, token), db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,i.company_id,i.owner_id,'legal',i.id,i.version,'Official law source changed; review original, text, dates and Company applicability before use',? FROM legal_items i WHERE i.id=? AND ?=false AND EXISTS(SELECT 1 FROM legal_members m WHERE m.item_id=i.id AND m.user_id=i.owner_id) ON CONFLICT DO NOTHING").bind(`law-source-notice-${job.id}`, now, job.rule_id, unchanged)]);
  } catch (error) {
    await db.prepare("UPDATE legal_law_source_checks SET state='failed',checked_at=NULL,error=?,lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?").bind(error instanceof Error && "code" in error ? String(error.code) : error instanceof Error && error.message === "LEGAL_SOURCE_TOO_LARGE" ? error.message : "LEGAL_SOURCE_CHECK_FAILED", job.id, token).run();
  }
}
