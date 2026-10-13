import { beforeAll, beforeEach, it, expect, vi } from "vitest";
import { PDFDocument } from "pdf-lib";
import { env, applyMigrations } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { legalCommand, detail } from "../../../src/server/modules/legal/public/legal/service";
import { access } from "../../../src/server/modules/legal/public/legal/access";
import { inspectDocument, uploadDocument } from "../../../src/server/modules/legal/public/legal/documents";
import { legalCitation } from "../../../src/server/modules/legal/public/legal/citations";
import { bangkokSourceCheckDate, drainLegalSourceChecks, lawCorpusCommand, lawCorpusView, officialLawUrl } from "../../../src/server/modules/legal/public/legal/law-corpus";
import { lawCitation, lawKnowledge, legalKnowledge, retrieveLegalAiEvidence, revalidateLegalAiEvidence, validateLegalAiCitations } from "../../../src/server/modules/legal/public/legal/knowledge";
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({ runtimeEnv: async () => env, holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }) }));
vi.mock("../../../src/server/modules/files/public/attachments/scanner", () => ({
  attachmentScannerReadiness: async () => ({ enabled: true, ready: true, engine: "simulated scanner", definitionUpdatedAt: Date.now(), error: null }),
  scanAttachmentBytes: async () => ({ clean: true, signature: null, engine: "simulated scanner", definitionUpdatedAt: Date.now() }),
}));
vi.mock("../../../src/server/modules/files/public/attachments/preview", () => ({ attachmentPreviewReadiness: async () => ({ ready: true, error: null }), detectedAttachmentMediaType: (_bytes: Uint8Array, type: string) => type, sanitizedAttachmentPreview: async () => null }));
const db = env.H001_DB, key = () => crypto.randomUUID(), text = "ข้อความสมมติสำหรับการทดสอบ สิทธิขอสำเนาข้อมูลส่วนบุคคล ต้องตรวจฉบับและข้อเท็จจริง ไม่ใช่ข้อสรุปกฎหมายจริง";
let f: Awaited<ReturnType<typeof seedExpenseCompany>>;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  await db.prepare("UPDATE legal_settings SET enabled=false").run();
  f = await seedExpenseCompany(db, env.H001_FILES);
  f.manager = await f.member("corpus-editor", "legal_manager");
  f.tie = await f.member("corpus-reviewer", "legal_manager");
  await legalCommand(db, f.manager, { action: "settings", enabled: true, aiEnabled: true }, key());
});
async function command(id: string, action: string, extra: Record<string, unknown> = {}, actor: WorkContext = f.manager, commandKey = key()) {
  return lawCorpusCommand(db, actor, { action, id, expectedVersion: (await access(db, actor, id)).version, reason: "Synthetic evidence review for engineering UAT", ...extra }, commandKey);
}
async function fixture(extra: Record<string, unknown> = {}) {
  const item = await legalCommand(db, f.manager, { action: "create", item: { kind: "rule", title: "Synthetic official-source review", classification: "internal", ownerId: f.manager.userId, data: { source: "https://ratchakitcha.soc.go.th/documents/synthetic.pdf", effectiveDate: "2026-01-01", applicability: "Synthetic Company facts require independent review", procedure: "Test evidence; no real legal conclusion" }, ...extra } }, key());
  await legalCommand(db, f.manager, { action: "member", id: item.id, expectedVersion: (await access(db, f.manager, item.id)).version, userId: f.tie.userId, access: "read" }, key());
  await legalCommand(db, f.manager, { action: "document", id: item.id, expectedVersion: (await access(db, f.manager, item.id)).version, name: "Synthetic official original", body: text }, key());
  const document = (await detail(db, f.manager, item.id)).documents[0];
  const source = await command(item.id, "law.source.register", { title: "Synthetic Gazette source", sourceUrl: "https://ratchakitcha.soc.go.th/documents/synthetic.pdf" });
  const revision = await command(item.id, "law.revision.import", { sourceId: source.evidenceId, documentId: document.id, revisionLabel: "Synthetic-v1", publishedOn: "2026-01-01", effectiveFrom: "2026-01-01", extractionMethod: "manual", chunks: [{ page: 2, section: "Test section 1", text }] });
  return { id: item.id, document, sourceId: source.evidenceId, revisionId: revision.evidenceId };
}
async function approveRevision(r: Awaited<ReturnType<typeof fixture>>) {
  await command(r.id, "law.revision.approve", { revisionId: r.revisionId, revisionVersion: 1, originalVerified: true, datesVerified: true, textVerified: true }, f.tie);
}
async function approveApplicability(r: Awaited<ReturnType<typeof fixture>>, applies = true) {
  const proposed = await command(r.id, "law.applicability.propose", { revisionId: r.revisionId, applies, basis: "Synthetic applicability assessment only" });
  await command(r.id, "law.applicability.approve", { applicabilityId: proposed.evidenceId, applicabilityVersion: 1 }, f.tie);
  return proposed.evidenceId;
}
it("accepts official government URLs and schedules once the Bangkok day reaches 06:00", () => {
  expect(officialLawUrl("https://www.dbd.go.th/path.pdf")).toBe("https://www.dbd.go.th/path.pdf");
  for (const url of ["http://dbd.go.th/a", "https://dbd.go.th.evil.test/a", "https://127.0.0.1/a", "https://user:password@dbd.go.th/a", "https://dbd.go.th:8443/a", "https://dbd.go.th/a#fragment"]) expect(() => officialLawUrl(url)).toThrow();
  expect(bangkokSourceCheckDate(Date.parse("2028-02-28T22:59:59Z"))).toBeNull();
  expect(bangkokSourceCheckDate(Date.parse("2028-02-28T23:00:00Z"))).toBe("2028-02-29");
});
it("keeps law text out of search until independent original and Company applicability review", async () => {
  const r = await fixture();
  expect((await legalKnowledge(db, f.manager, "สิทธิขอสำเนา")).sources).toHaveLength(0);
  await expect(command(r.id, "law.revision.approve", { revisionId: r.revisionId, revisionVersion: 1, originalVerified: true, datesVerified: true, textVerified: true })).rejects.toMatchObject({ code: "LEGAL_INDEPENDENT_REVIEW" });
  await approveRevision(r);
  expect((await lawKnowledge(db, f.manager, "สิทธิขอสำเนา", "2026-09-30")).sources).toHaveLength(0);
  await approveApplicability(r);
  const source = (await lawKnowledge(db, f.manager, "สิทธิขอสำเนา", "2026-09-30")).sources[0];
  expect(source).toMatchObject({ page: 2, section: "Test section 1", revision_id: r.revisionId, document_id: r.document.id, checksum: r.document.checksum, effective_from: "2026-01-01" });
  expect((await lawCitation(db, f.manager, source.id, "2026-09-30")).originalUrl).toContain("#page=2");
});
it("requires independent applicability review and supports a reviewed non-applicability decision", async () => {
  const r = await fixture(); await approveRevision(r);
  const applicabilityId = await command(r.id, "law.applicability.propose", { revisionId: r.revisionId, applies: false, basis: "Synthetic Company is outside the tested facts" });
  await expect(command(r.id, "law.applicability.approve", { applicabilityId: applicabilityId.evidenceId, applicabilityVersion: 1 })).rejects.toMatchObject({ code: "LEGAL_INDEPENDENT_REVIEW" });
  await command(r.id, "law.applicability.approve", { applicabilityId: applicabilityId.evidenceId, applicabilityVersion: 1 }, f.tie);
  expect((await lawKnowledge(db, f.manager, "สิทธิขอสำเนา")).sources).toHaveLength(0);
});
it("does not leak law chunks, corpus rows or citations across Company or confidential membership", async () => {
  const r = await fixture({ classification: "restricted" }); await approveRevision(r); await approveApplicability(r);
  const source = (await lawKnowledge(db, f.manager, "สิทธิขอสำเนา")).sources[0];
  const outsider = await f.member("outside-law-matter", "legal_counsel");
  expect((await lawKnowledge(db, outsider, "สิทธิขอสำเนา")).sources).toHaveLength(0);
  await expect(lawCitation(db, outsider, source.id)).rejects.toMatchObject({ code: "LEGAL_CITATION_UNAVAILABLE" });
  const other = await seedExpenseCompany(db, env.H001_FILES);
  expect((await lawCorpusView(db, other.owner)).sources).toHaveLength(0);
  await expect(lawCitation(db, other.owner, source.id)).rejects.toMatchObject({ code: "LEGAL_CITATION_UNAVAILABLE" });
});
it("enforces effective dates, excludes conflicting versions, and retains old deadlines", async () => {
  const r = await fixture(); await approveRevision(r); await approveApplicability(r);
  expect((await lawKnowledge(db, f.manager, "สิทธิขอสำเนา", "2025-12-31")).sources).toHaveLength(0);
  const before = await access(db, f.manager, r.id);
  const newer = await command(r.id, "law.revision.import", { sourceId: r.sourceId, documentId: r.document.id, revisionLabel: "Synthetic-v2", publishedOn: "2026-09-01", effectiveFrom: "2026-09-01", extractionMethod: "manual", chunks: [{ page: 2, section: "Test amendment", text }] });
  await approveRevision({ ...r, revisionId: newer.evidenceId }); await approveApplicability({ ...r, revisionId: newer.evidenceId });
  const result = await lawKnowledge(db, f.manager, "สิทธิขอสำเนา", "2026-09-30");
  expect(result.sources).toHaveLength(0); expect(result.conflicts).toBe(1);
  expect((await access(db, f.manager, r.id)).due_date).toBe(before.due_date);
  expect((await lawKnowledge(db, f.manager, "สิทธิขอสำเนา", "2026-08-31")).sources[0].revision_id).toBe(r.revisionId);
});
it("replays imports, rejects stale versions, and preserves immutable source chunks", async () => {
  const r = await fixture(), body = { action: "law.applicability.propose", id: r.id, expectedVersion: (await access(db, f.manager, r.id)).version, revisionId: r.revisionId, applies: true, basis: "Synthetic reviewed Company facts", reason: "Test" };
  await approveRevision(r);
  await expect(lawCorpusCommand(db, f.manager, body, key())).rejects.toMatchObject({ code: "LEGAL_VERSION_CONFLICT" });
  const commandKey = key(), current = { ...body, expectedVersion: (await access(db, f.manager, r.id)).version };
  const first = await lawCorpusCommand(db, f.manager, current, commandKey);
  expect(await lawCorpusCommand(db, f.manager, current, commandKey)).toEqual(first);
  await expect(db.prepare("UPDATE legal_source_chunks SET text='replaced' WHERE revision_id=?").bind(r.revisionId).run()).rejects.toThrow("LEGAL_EVIDENCE_IMMUTABLE");
  await expect(db.prepare("UPDATE legal_law_revisions SET effective_from='2027-01-01' WHERE id=?").bind(r.revisionId).run()).rejects.toThrow("LEGAL_CORPUS_REVISION_IMMUTABLE");
});
it("keeps source fetch failure visible and new source downloads pending, with restart deduplication", async () => {
  const r = await fixture(), now = Date.parse("2026-10-01T00:00:00Z"), fetcher = vi.fn(async () => new Response("Synthetic updated official source"));
  await drainLegalSourceChecks(db, env.H001_FILES, fetcher, now);
  const checks = (await lawCorpusView(db, f.manager, r.id)).checks;
  expect(checks[0]).toMatchObject({ state: "pending_review", check_date: "2026-10-01" });
  await drainLegalSourceChecks(db, env.H001_FILES, fetcher, now);
  expect(fetcher).toHaveBeenCalledTimes(1);
  await drainLegalSourceChecks(db, env.H001_FILES, async () => new Response(null, { status: 503 }), now + 86400000);
  const failed = (await lawCorpusView(db, f.manager, r.id)).checks.find((row) => row.check_date === "2026-10-02");
  expect(failed).toMatchObject({ state: "failed", checked_at: null, error: "LEGAL_SOURCE_HTTP_503" });
  expect((await lawKnowledge(db, f.manager, "สิทธิขอสำเนา")).sources).toHaveLength(0);
});
it("rejects unsupported citations and quotations and keeps verified quotations pending legal review", async () => {
  const r = await fixture(); await approveRevision(r); await approveApplicability(r);
  const evidence = await retrieveLegalAiEvidence(db, f.manager, r.id, "สิทธิขอสำเนา"), sourceId = evidence.sources[0].sourceId;
  expect(() => validateLegalAiCitations({ sources: ["invented"], claims: [] }, evidence)).toThrow();
  expect(() => validateLegalAiCitations({ sources: [sourceId], claims: [{ text: "Unsupported claim", sources: [sourceId], quotes: [{ sourceId, quote: "Invented quoted provision" }] }] }, evidence)).toThrow();
  expect(() => validateLegalAiCitations({ sources: [sourceId], claims: [{ text: "Citation without support", sources: [sourceId] }] }, evidence)).toThrow();
  const verified = validateLegalAiCitations({ sources: [sourceId], claims: [{ text: "Synthetic claim requiring review", sources: [sourceId], quotes: [{ sourceId, quote: "สิทธิขอสำเนาข้อมูลส่วนบุคคล" }] }] }, evidence);
  expect(verified.claims[0]).toMatchObject({ supportStatus: "quotation_verified_pending_legal_review", requiresHumanReview: true });
  expect(verified.citations[0]).toMatchObject({ page: 2, revision_id: r.revisionId });
  expect(validateLegalAiCitations({ sources: [], claims: [{ text: "Evidence is insufficient", sources: [] }] }, evidence).evidenceStatus).toBe("insufficient_evidence");
});
it("rechecks live membership and retirement after evidence selection", async () => {
  const r = await fixture({ classification: "restricted" }); await approveRevision(r); await approveApplicability(r);
  const evidence = await retrieveLegalAiEvidence(db, f.manager, r.id, "สิทธิขอสำเนา");
  await db.prepare("DELETE FROM legal_members WHERE item_id=? AND user_id=?").bind(r.id, f.manager.userId).run();
  await expect(revalidateLegalAiEvidence(db, f.manager, evidence)).rejects.toMatchObject({ code: "LEGAL_CITATION_UNAVAILABLE" });
});
it("requires matching OCR inspection checksum before an extraction can be submitted", async () => {
  const r = await fixture();
  await expect(command(r.id, "law.revision.import", { sourceId: r.sourceId, documentId: r.document.id, revisionLabel: "OCR-v2", publishedOn: "2026-09-01", effectiveFrom: "2026-09-01", extractionMethod: "ocr", chunks: [{ page: 1, text }] })).rejects.toThrow("Select the extraction inspection");
  const inspectionId = key();
  await db.prepare("INSERT INTO legal_document_inspections VALUES(?,?,'ocr',?,?,?)").bind(inspectionId, r.document.id, JSON.stringify({ checksum: "different-original", text, requiresReview: true }), f.manager.userId, Date.now()).run();
  await expect(command(r.id, "law.revision.import", { sourceId: r.sourceId, documentId: r.document.id, revisionLabel: "OCR-v2", publishedOn: "2026-09-01", effectiveFrom: "2026-09-01", extractionMethod: "ocr", inspectionId, chunks: [{ page: 1, text }] })).rejects.toMatchObject({ code: "LEGAL_SOURCE_CHECKSUM" });
});
it("reviews extracted document text independently and does not invent printed pagination for logical chunks", async () => {
  const item = await legalCommand(db, f.manager, { action: "create", item: { kind: "contract", title: "Synthetic Thai source review", classification: "internal", ownerId: f.manager.userId, data: { contractType: "Synthetic", parties: "Company and fixture" } } }, key());
  await legalCommand(db, f.manager, { action: "document", id: item.id, expectedVersion: (await access(db, f.manager, item.id)).version, name: "Synthetic authored text", body: text }, key());
  const document = (await detail(db, f.manager, item.id)).documents[0], inspection = await inspectDocument(db, env.H001_FILES, f.manager, document.id, "ocr");
  const review = await command(item.id, "knowledge.document.import", { documentId: document.id, extractionMethod: "ocr", inspectionId: inspection.inspectionId, chunks: [{ page: 1, section: "Synthetic clause", text }] });
  expect((await legalKnowledge(db, f.manager, "สิทธิขอสำเนา")).sources.filter((source) => source.sourceId.startsWith("document:"))).toHaveLength(0);
  await expect(command(item.id, "knowledge.document.approve", { textReviewId: review.evidenceId, textReviewVersion: 1, textVerified: true })).rejects.toMatchObject({ code: "LEGAL_INDEPENDENT_REVIEW" });
  await command(item.id, "knowledge.document.approve", { textReviewId: review.evidenceId, textReviewVersion: 1, textVerified: true }, f.tie);
  const source = (await legalKnowledge(db, f.manager, "สิทธิขอสำเนา")).sources.find((source) => source.sourceId.startsWith("document:"));
  expect(source).toMatchObject({ page: null, page_mapping: false, section: "Synthetic clause", document_id: document.id, checksum: document.checksum });
  expect(await legalCitation(db, f.manager, source!.sourceId)).toMatchObject({ page: null, page_mapping: false, section: "Synthetic clause", text });
  expect((await lawCorpusView(db, f.manager, item.id)).chunks[0]).toMatchObject({ page: 1, page_mapping: false });
});
it("rechecks direct document citation membership and Company scope after the passage was selected", async () => {
  const item = await legalCommand(db, f.manager, { action: "create", item: { kind: "contract", title: "Confidential citation fixture", classification: "restricted", ownerId: f.manager.userId, data: { contractType: "Synthetic", parties: "Company and fixture" } } }, key());
  await legalCommand(db, f.manager, { action: "document", id: item.id, expectedVersion: (await access(db, f.manager, item.id)).version, name: "Confidential original", body: text }, key());
  await legalCommand(db, f.manager, { action: "member", id: item.id, expectedVersion: (await access(db, f.manager, item.id)).version, userId: f.tie.userId, access: "read" }, key());
  const document = (await detail(db, f.manager, item.id)).documents[0];
  const review = await command(item.id, "knowledge.document.import", { documentId: document.id, extractionMethod: "manual", chunks: [{ page: 1, section: "Confidential clause", text }] });
  await command(item.id, "knowledge.document.approve", { textReviewId: review.evidenceId, textReviewVersion: 1, textVerified: true }, f.tie);
  const source = (await legalKnowledge(db, f.tie, "สิทธิขอสำเนา")).sources.find((entry) => entry.sourceId.startsWith("document:"))!;
  expect(await legalCitation(db, f.tie, source.sourceId)).toMatchObject({ document_id: document.id, text });
  const other = await seedExpenseCompany(db, env.H001_FILES);
  await expect(legalCitation(db, other.owner, source.sourceId)).rejects.toMatchObject({ code: "LEGAL_CITATION_UNAVAILABLE" });
  await db.prepare("DELETE FROM legal_members WHERE item_id=? AND user_id=?").bind(item.id, f.tie.userId).run();
  await expect(legalCitation(db, f.tie, source.sourceId)).rejects.toMatchObject({ code: "LEGAL_NOT_FOUND" });
});
it("withholds a previously approved file passage when its original is revoked", async () => {
  const item = await legalCommand(db, f.manager, { action: "create", item: { kind: "contract", title: "Scanned citation fixture", classification: "internal", ownerId: f.manager.userId, data: { contractType: "Synthetic", parties: "Company and fixture" } } }, key());
  const pdf = await PDFDocument.create(); pdf.addPage();
  await uploadDocument(db, env.H001_FILES, f.manager, item.id, (await access(db, f.manager, item.id)).version, "Synthetic original.pdf", "application/pdf", await pdf.save(), key());
  const document = (await detail(db, f.manager, item.id)).documents[0];
  const review = await command(item.id, "knowledge.document.import", { documentId: document.id, extractionMethod: "manual", chunks: [{ page: 1, section: "Synthetic reviewed passage", text }] });
  await command(item.id, "knowledge.document.approve", { textReviewId: review.evidenceId, textReviewVersion: 1, textVerified: true }, f.tie);
  const source = (await legalKnowledge(db, f.manager, "สิทธิขอสำเนา")).sources.find((entry) => entry.sourceId.startsWith("document:"))!;
  expect(await legalCitation(db, f.manager, source.sourceId)).toMatchObject({ page: 1, page_mapping: true, text });
  const attachmentId = await db.prepare("SELECT attachment_id FROM attachment_links WHERE id=?").bind(document.attachment_link_id).first<string>("attachment_id");
  await db.prepare("UPDATE attachments SET revoked_at=? WHERE id=?").bind(Date.now(), attachmentId).run();
  await expect(legalCitation(db, f.manager, source.sourceId)).rejects.toMatchObject({ code: "LEGAL_CITATION_UNAVAILABLE" });
  expect((await legalKnowledge(db, f.manager, "สิทธิขอสำเนา")).sources).toHaveLength(0);
});
it("skips source-fetch side effects when Company Legal is disabled", async () => {
  await fixture();
  await legalCommand(db, f.manager, { action: "settings", enabled: false }, key());
  const fetcher = vi.fn(async () => new Response("Synthetic official source"));
  await drainLegalSourceChecks(db, env.H001_FILES, fetcher, Date.parse("2026-10-01T00:00:00Z"));
  expect(fetcher).not.toHaveBeenCalled();
  expect((await lawCorpusView(db, f.manager)).checks).toHaveLength(0);
});
it("binds a daily-source candidate to the exact uploaded original before marking its review complete", async () => {
  const r = await fixture();
  await drainLegalSourceChecks(db, env.H001_FILES, async () => new Response(text), Date.parse("2026-10-01T00:00:00Z"));
  const check = (await lawCorpusView(db, f.manager, r.id)).checks[0];
  const candidate = await command(r.id, "law.revision.import", { sourceId: r.sourceId, sourceCheckId: check.id, documentId: r.document.id, revisionLabel: "Daily-candidate", publishedOn: "2026-10-01", effectiveFrom: "2026-10-01", extractionMethod: "manual", chunks: [{ page: 2, section: "Synthetic daily revision", text }] });
  expect((await lawCorpusView(db, f.manager, r.id)).checks[0]).toMatchObject({ state: "pending_review", candidate_revision_id: candidate.evidenceId });
  await approveRevision({ ...r, revisionId: candidate.evidenceId });
  expect((await lawCorpusView(db, f.manager, r.id)).checks[0]).toMatchObject({ state: "reviewed", checksum: r.document.checksum });
  expect((await lawKnowledge(db, f.manager, "สิทธิขอสำเนา", "2026-10-01")).sources).toHaveLength(0);
});
