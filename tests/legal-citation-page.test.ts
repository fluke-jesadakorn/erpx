import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { isValidElement, type ReactNode } from "react";
import { getDynamicParam } from "next/dist/shared/lib/router/utils/get-dynamic-param";
import type { WorkContext } from "../src/server/authorization";
import { applyMigrations, env } from "./runtime-env";
import { seedExpenseCompany } from "../fixtures/expenses/company";
import { access } from "../src/server/legal/access";
import { detail, legalCommand } from "../src/server/legal/service";
import { lawCorpusCommand } from "../src/server/legal/law-corpus";
import { legalKnowledge } from "../src/server/legal/knowledge";

const boundary = vi.hoisted(() => ({ actor: null as WorkContext | null }));
vi.mock("../src/server/authorization", async original => ({
  ...await original<typeof import("../src/server/authorization")>(),
  requireWorkContext: async () => boundary.actor!,
}));
vi.mock("../src/server/runtime", async original => ({
  ...await original<typeof import("../src/server/runtime")>(),
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env,
}));
// The request boundary is the isolated fixture. Native page loading and all
// Legal source revision/date/member/Company guards remain real.
vi.mock("../src/server/modules/work_projection/public/page-error", () => ({ pageError: async (error: unknown) => { throw error; } }));
import CitationPage from "../src/app/holdings/[holdingSlug]/legal/citations/[id]/page";

const db = env.H001_DB, key = () => crypto.randomUUID(), text = "ข้อความอ้างอิงสมมติสำหรับการทดสอบ ไม่ใช่กฎหมายหรือคำแนะนำจริง";
let f: Awaited<ReturnType<typeof seedExpenseCompany>>;
beforeAll(() => applyMigrations(db, env.STAFF_TEST_MIGRATIONS));
beforeEach(async () => {
  f = await seedExpenseCompany(db, env.H001_FILES); f.manager = await f.member("source-author", "legal_manager"); f.tie = await f.member("source-reviewer", "legal_manager");
  boundary.actor = f.tie;
  await legalCommand(db, f.manager, { action: "settings", enabled: true }, key());
});
function command(id:string,action:`law.${string}`|`knowledge.${string}`,extra?:Record<string,unknown>,actor?:WorkContext):ReturnType<typeof lawCorpusCommand>;
function command(id:string,action:string,extra?:Record<string,unknown>,actor?:WorkContext):ReturnType<typeof lawCorpusCommand>|ReturnType<typeof legalCommand>;
async function command(id: string, action: string, extra: Record<string, unknown> = {}, actor: WorkContext = f.manager) {
  const input = { action, id, expectedVersion: (await access(db, actor, id)).version, reason: "Independent synthetic engineering evidence review", ...extra };
  return action.startsWith("law.") || action.startsWith("knowledge.") ? lawCorpusCommand(db, actor, input, key()) : legalCommand(db, actor, input, key());
}
async function original(kind: "rule" | "contract") {
  const created = await legalCommand(db, f.manager, { action: "create", item: { kind, classification: "restricted", title: "Synthetic exact citation", ownerId: f.manager.userId, data: kind === "rule" ? { source: "https://ratchakitcha.soc.go.th/synthetic-page-test.pdf", effectiveDate: "2026-01-01", applicability: "Only fictional Company facts", procedure: "Engineering fixture only" } : { contractType: "Synthetic", parties: "Company and fictional supplier" } } }, key());
  await command(created.id, "member", { userId: f.tie.userId, access: "read" });
  await command(created.id, "document", { name: "Immutable fictional original", body: text });
  return { itemId: created.id, document: (await detail(db, f.manager, created.id)).documents[0] };
}
function pageParam(sourceId: string) {
  const parameter = getDynamicParam({ id: sourceId }, "id", "d", null, null);
  expect(parameter.value).toBe(encodeURIComponent(sourceId));
  return parameter.value as string;
}
const page = (sourceId: string, asOf = "2026-09-30") => CitationPage({ params: Promise.resolve({ holdingSlug: "test", id: pageParam(sourceId) }), searchParams: Promise.resolve({ asOf }) });
function element(node: ReactNode, tag: string): { children?: ReactNode; href?: string } | undefined {
  if (Array.isArray(node)) return node.map(child => element(child, tag)).find(Boolean);
  if (!isValidElement<{ children?: ReactNode; href?: string }>(node)) return;
  return node.type === tag ? node.props : element(node.props.children, tag);
}

it("opens an encoded reviewed law page from the installed Next params and retains exact dates, original checksum and current scope", async () => {
  const record = await original("rule"), source = await command(record.itemId, "law.source.register", { title: "Declared fictional Gazette URL fixture", sourceUrl: "https://ratchakitcha.soc.go.th/synthetic-page-test.pdf" });
  const revision = await command(record.itemId, "law.revision.import", { sourceId: source.evidenceId, documentId: record.document.id, revisionLabel: "Fictional-v1", publishedOn: "2026-01-01", effectiveFrom: "2026-01-01", extractionMethod: "manual", chunks: [{ page: 1, section: "Synthetic retained clause", text }] });
  await command(record.itemId, "law.revision.approve", { revisionId: revision.evidenceId, revisionVersion: 1, originalVerified: true, datesVerified: true, textVerified: true }, f.tie);
  const applicability = await command(record.itemId, "law.applicability.propose", { revisionId: revision.evidenceId, applies: true, basis: "Fictional Company engineering facts only" });
  await command(record.itemId, "law.applicability.approve", { applicabilityId: applicability.evidenceId, applicabilityVersion: 1 }, f.tie);
  const passage = (await legalKnowledge(db, f.tie, "ข้อความอ้างอิงสมมติ", "2026-09-30")).sources.find(row => row.kind === "law")!;
  expect(passage).toMatchObject({ checksum: record.document.checksum, page: 1, revision_id: revision.evidenceId });
  expect(element(await page(passage.sourceId), "blockquote")?.children).toBe(text);
  await expect(page(passage.sourceId, "2025-12-31")).rejects.toMatchObject({ status: 404, code: "LEGAL_CITATION_UNAVAILABLE" });
  const foreign = await seedExpenseCompany(db, env.H001_FILES); boundary.actor = await foreign.member("foreign-source-reviewer", "legal_manager");
  await expect(page(passage.sourceId)).rejects.toMatchObject({ status: 404, code: "LEGAL_CITATION_UNAVAILABLE" });
  boundary.actor = f.tie; await command(record.itemId, "member", { userId: f.tie.userId, access: "remove" });
  await expect(page(passage.sourceId)).rejects.toMatchObject({ status: 404, code: "LEGAL_CITATION_UNAVAILABLE" });
  expect((await detail(db, f.manager, record.itemId)).documents[0].checksum).toBe(record.document.checksum);
});

it("opens an encoded independently reviewed document passage and removes both content and links after native membership withdrawal", async () => {
  const record = await original("contract"), review = await command(record.itemId, "knowledge.document.import", { documentId: record.document.id, extractionMethod: "manual", chunks: [{ page: 1, pageMapping: false, section: "Fictional independently reviewed clause", text }] });
  await command(record.itemId, "knowledge.document.approve", { textReviewId: review.evidenceId, textReviewVersion: 1, textVerified: true }, f.tie);
  const passage = (await legalKnowledge(db, f.tie, "ข้อความอ้างอิงสมมติ")).sources.find(row => row.sourceId.startsWith("document:"))!;
  expect(passage).toMatchObject({ document_id: record.document.id, checksum: record.document.checksum, page: null });
  const rendered = await page(passage.sourceId); expect(element(rendered, "blockquote")?.children).toBe(text);
  expect(element(rendered, "a")?.href).toBe(`/api/v1/holdings/test/legal/documents/${record.document.id}`);
  await command(record.itemId, "member", { userId: f.tie.userId, access: "remove" });
  await expect(page(passage.sourceId)).rejects.toMatchObject({ status: 404, code: "LEGAL_NOT_FOUND" });
  expect((await legalKnowledge(db, f.tie, "ข้อความอ้างอิงสมมติ")).sources).toEqual([]);
});
