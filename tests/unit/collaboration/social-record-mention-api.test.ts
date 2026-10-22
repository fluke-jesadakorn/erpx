import { beforeEach, expect, it, vi } from "vitest";
const owner = vi.hoisted(() => ({ current: vi.fn(), read: vi.fn(), context: vi.fn() }));
vi.mock("@erpx/server-identity_access/public/authorization", () => ({ requireWorkContext: owner.current }));
vi.mock("@erpx/server-records/adapters/records", () => ({ getRecord: owner.read, recordContext: owner.context }));
import { GET } from "@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/records/[module]/[id]/route";
import { ApiError } from "@erpx/server-runtime/public/http";

const actor = { companyId: "current-company", assignmentId: "current-role", authority: { revision: 17 } };
const record = { title: "Retained title", description: "Retained body", status: "approved", version: 2,
  fields: [{ key: "f", label: "F", type: "text", value: "display", options: ["private options"] }],
  moduleId: "finance", id: "old-record", sectionId: "old-section", createdBy: "private-author", updatedAt: 1 };
const params = { params: Promise.resolve({ holdingSlug: "acme", module: "finance", id: "old-record" }) };
beforeEach(() => { vi.clearAllMocks(); owner.current.mockResolvedValue(actor); owner.read.mockResolvedValue(record); owner.context.mockResolvedValue({ record, suggestions: [{ provenance: "original private context" }] }); });

it("projects actual owner-authorized record fields before the mention response crosses the API boundary", async () => {
  const response = await GET(new Request("https://work-dev-erpx.jesadakorn.com/api/v1/holdings/acme/records/finance/old-record?view=mention"), params);
  expect(response.status).toBe(200); expect(response.headers.get("cache-control")).toBe("private, no-store");
  expect(owner.current).toHaveBeenCalledWith("acme"); expect(owner.read).toHaveBeenCalledWith(actor, "finance", "old-record"); expect(owner.context).not.toHaveBeenCalled();
  expect(await response.json()).toEqual({ data: { record: { title: record.title, description: record.description, status: record.status, version: 2, fields: [{ key: "f", label: "F", value: "display" }] } } });
});

it("retains original full record-context responses for existing callers", async () => {
  const response = await GET(new Request("https://console-dev-erpx.jesadakorn.com/api/v1/holdings/acme/records/finance/old-record"), params);
  expect(response.status).toBe(200); expect(owner.read).not.toHaveBeenCalled(); expect(owner.context).toHaveBeenCalledWith(actor, "finance", "old-record");
  expect((await response.json()).data.suggestions).toEqual([{ provenance: "original private context" }]);
});

it("does not convert owner purchase or current-role denial into a readable mention", async () => {
  for (const [status, code] of [[403, "MODULE_FORBIDDEN"], [404, "NOT_FOUND"], [409, "WORK_CONTEXT_CHANGED"]] as const) {
    owner.read.mockRejectedValueOnce(new ApiError(status, code, "Original owner refusal"));
    const response = await GET(new Request("https://work-dev-erpx.jesadakorn.com/api/v1/holdings/acme/records/finance/old-record?view=mention"), params);
    expect(response.status).toBe(status); expect((await response.json()).error.code).toBe(code);
  }
});
