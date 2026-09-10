import { expect, it, vi, afterEach } from "vitest";
import { mergeAiDraft, type RecoverableDraft } from "../src/lib/ai-draft";
import { createReceiptUploader, validateReceipt } from "../src/lib/expense-upload";
import { prepareDocumentPages } from "../src/lib/document-pages";
vi.mock("../src/lib/document-pages", () => ({ prepareDocumentPages: vi.fn(async () => {}) }));
afterEach(() => { vi.unstubAllGlobals(); vi.clearAllMocks(); });
const baseline = JSON.stringify({ merchant: "", currency: "", amount: "" });
const initial = (): RecoverableDraft<Record<string, string>> => ({ fields: JSON.parse(baseline), baseline, ai: {} });
const event = { proposalId: "proposal-1", overwrite: false, fields: [{ name: "merchant", value: "ร้านทดสอบ", citations: [{ attachmentId: "receipt-1", page: 2 }] }, { name: "currency", value: "THB" }] };
it("fills empty receipt fields and retains source page references", () => {
  const draft = mergeAiDraft(initial(), baseline, event);
  expect(draft.fields).toMatchObject({ merchant: "ร้านทดสอบ", currency: "THB" });
  expect(draft.ai.merchant.citations).toEqual([{ attachmentId: "receipt-1", page: 2 }]);
});
it("preserves both typed values and fields explicitly cleared while AI runs", () => {
  const draft = initial(); draft.fields.merchant = "My merchant"; draft.touched = ["currency"];
  expect(mergeAiDraft(draft, baseline, event).fields).toMatchObject({ merchant: "My merchant", currency: "" });
});
it("replaces only on explicit use-suggestion and ignores unknown/null fields", () => {
  const draft = initial(); draft.fields.merchant = "Mine"; draft.touched = ["merchant"];
  expect(mergeAiDraft(draft, baseline, { ...event, overwrite: true }).fields.merchant).toBe("ร้านทดสอบ");
  const result = mergeAiDraft(draft, baseline, { ...event, fields: [{ name: "personId", value: "other" }, { name: "amount", value: null }] });
  expect(result.fields).not.toHaveProperty("personId"); expect(result.fields.amount).toBe("");
});
it("invalidates stale AI fields while keeping human edits and accepts new evidence", () => {
  const draft = mergeAiDraft(initial(), baseline, event);
  draft.fields.merchant = "Edited by human"; draft.touched = ["merchant"]; delete draft.ai.merchant;
  const stale = mergeAiDraft(draft, baseline, { fields: [], overwrite: false, invalidate: true, proposalId: null });
  expect(stale.fields).toMatchObject({ merchant: "Edited by human", currency: "" });
  const fresh = mergeAiDraft(stale, baseline, { ...event, proposalId: "proposal-2" });
  expect(fresh.fields).toMatchObject({ merchant: "Edited by human", currency: "THB" });
});
it("retains provenance after browser recovery and does not invalidate the same proposal", () => {
  const draft = JSON.parse(JSON.stringify(mergeAiDraft(initial(), baseline, event)));
  expect(mergeAiDraft(draft, baseline, { ...event, invalidate: true })).toBe(draft);
});
it("rejects unsupported or empty files before uploading", () => {
  expect(() => validateReceipt(new File([], "empty.png", { type: "image/png" }))).toThrow();
  expect(() => validateReceipt(new File(["x"], "x.exe", { type: "application/octet-stream" }))).toThrow();
});
it("retries an interrupted PUT with the same intent and avoids duplicate successful uploads", async () => {
  const file = new File(["synthetic"], "receipt.png", { type: "image/png" });
  const fetcher = vi.fn().mockResolvedValueOnce(Response.json({ data: { uploadUrl: "/upload/same" } }))
    .mockRejectedValueOnce(Error("Network interrupted"))
    .mockResolvedValueOnce(Response.json({ data: { id: "receipt-1", checksum: "sha" } }));
  vi.stubGlobal("fetch", fetcher);
  const upload = createReceiptUploader();
  await expect(upload("/holdings/acme", "claim-1", file)).rejects.toThrow("Network interrupted");
  await upload("/holdings/acme", "claim-1", file);
  await upload("/holdings/acme", "claim-1", file);
  expect(fetcher).toHaveBeenCalledTimes(3);
  expect(fetcher.mock.calls[1][0]).toBe(fetcher.mock.calls[2][0]);
  expect(prepareDocumentPages).toHaveBeenCalledTimes(1);
});
it("retries failed PDF rendering without uploading another original", async () => {
  const file = new File(["synthetic"], "receipt.pdf", { type: "application/pdf" });
  const fetcher = vi.fn().mockResolvedValueOnce(Response.json({ data: { uploadUrl: "/upload/pdf" } }))
    .mockResolvedValueOnce(Response.json({ data: { id: "receipt-2", checksum: "sha" } }));
  vi.stubGlobal("fetch", fetcher);
  vi.mocked(prepareDocumentPages).mockRejectedValueOnce(Error("Unreadable page"));
  const upload = createReceiptUploader();
  await expect(upload("/holdings/acme", "claim-1", file)).rejects.toThrow("The original receipt is saved");
  await upload("/holdings/acme", "claim-1", file);
  expect(fetcher).toHaveBeenCalledTimes(2);
});

it("recovers a lost draft response after reload with the original idempotency key and values", async () => {
  const { createExpenseDraftOnce } = await import("../src/lib/expense-draft-create");
  const stored = new Map<string, string>();
  vi.stubGlobal("sessionStorage", { getItem: (key: string) => stored.get(key) ?? null, setItem: (key: string, value: string) => stored.set(key, value), removeItem: (key: string) => stored.delete(key) });
  const fetcher = vi.fn().mockRejectedValueOnce(Error("Response lost"))
    .mockResolvedValueOnce(Response.json({ data: { id: "same-draft" } }));
  vi.stubGlobal("fetch", fetcher);
  await expect(createExpenseDraftOnce()("/holdings/acme", "draft:new", { merchant: "Typed before upload", currency: "" }, ["currency"])).rejects.toThrow("Response lost");
  const restored = createExpenseDraftOnce();
  const first = restored("/holdings/acme", "draft:new", {});
  const second = restored("/holdings/acme", "draft:new", {});
  expect(first).toBe(second);
  expect(await first).toBe("same-draft");
  expect(fetcher.mock.calls[1][1]).toEqual(fetcher.mock.calls[0][1]);
  expect(stored.get("draft:new:created")).toBe("same-draft");
  expect(stored.has("draft:new:creation")).toBe(false);
  expect(JSON.parse(stored.get("draft:same-draft:touched")!)).toEqual(["currency"]);
});
it("allows correcting a definitively rejected draft without replaying invalid input", async () => {
  const { createExpenseDraftOnce } = await import("../src/lib/expense-draft-create");
  const fetcher = vi.fn().mockResolvedValueOnce(Response.json({ error: { message: "Invalid date" } }, { status: 422 }))
    .mockResolvedValueOnce(Response.json({ data: { id: "corrected-draft" } }));
  vi.stubGlobal("fetch", fetcher);
  const create = createExpenseDraftOnce();
  await expect(create("/holdings/acme", "new", { expenseDate: "invalid" })).rejects.toThrow("Invalid date");
  await create("/holdings/acme", "new", { expenseDate: "2026-09-09" });
  expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({ expenseDate: "2026-09-09" });
});

it("fills the receipt total despite unknown VAT recoverability and preserves user amounts", async () => {
  const { normalizeDraftReceiptAmounts } = await import("../src/lib/expense-ai");
  const citations = [{ attachmentId: "swiss", page: 1 }];
  const result = normalizeDraftReceiptAmounts({ summary: "CHF 54.50 includes VAT 3.85", fields: [
    { name: "receiptTotal", value: "54.50", citations }, { name: "amount", value: "50.65", citations }, { name: "tax", value: "3.85", citations },
  ], missing: ["amount", "tax"], conflicts: [] });
  expect(result.fields.find(f => f.name === "amount")?.value).toBe("54.50");
  expect(result.fields.find(f => f.name === "tax")?.value).toBeNull();
  expect(result.missing).toEqual(["tax"]);
  const fields = { amount: "", tax: "", currency: "CHF" }, snapshot = JSON.stringify(fields);
  const draft = mergeAiDraft({ fields, baseline: snapshot, ai: {} }, snapshot, { fields: result.fields, overwrite: false, proposalId: "swiss" });
  expect(draft.fields.amount).toBe("54.50"); expect(draft.fields.tax).toBe("");
  expect(draft.ai.amount.citations).toEqual(citations);
  draft.fields.amount = "49.00"; delete draft.ai.amount;
  expect(mergeAiDraft(draft, snapshot, { fields: result.fields, overwrite: false, proposalId: "swiss-2" }).fields.amount).toBe("49.00");
});
it("uses a recoverable tax edited while AI was running without counting it twice", () => {
  const fields = { amount: "", tax: "3.85" }, snapshot = JSON.stringify({ amount: "", tax: "" });
  const result = mergeAiDraft({ fields, baseline: snapshot, ai: {}, touched: ["tax"] }, snapshot, { overwrite: false, proposalId: "receipt", fields: [
    { name: "receiptTotal", value: "54.50" }, { name: "amount", value: "54.50" }, { name: "tax", value: null },
  ] });
  expect(result.fields).toEqual({ amount: "50.65", tax: "3.85" });
});
it.each([null, "-54.50", "999999999999999999999", "invalid"])("does not turn missing or invalid receipt total %s into an amount", async value => {
  const { normalizeDraftReceiptAmounts } = await import("../src/lib/expense-ai");
  const result = normalizeDraftReceiptAmounts({ summary: "Conflicting evidence", fields: [
    { name: "receiptTotal", value, citations: [{ attachmentId: "r", page: 1 }] },
    { name: "amount", value: "50.65", citations: [{ attachmentId: "r", page: 1 }] },
  ], missing: [], conflicts: [] });
  expect(result.fields.find(f => f.name === "amount")?.value).toBeNull();
});
