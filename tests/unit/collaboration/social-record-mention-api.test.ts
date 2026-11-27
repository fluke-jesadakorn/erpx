import { expect, it, vi } from "vitest";
import { createMentionRecordRead } from "@erpx/feature-records/lib/mention-read";

const base = "/holdings/acme";
const record = { title: "Record title", description: "Record body", status: "approved", version: 2,
  fields: [{ key: "f", label: "F", type: "text", value: "display", options: ["private options"] }],
  moduleId: "finance", id: "old-record", sectionId: "old-section", createdBy: "private-author", updatedAt: 1 };

it("projects display fields from the native context response without exposing related data in the drawer", async () => {
  const fetcher = vi.fn(async () => Response.json({
    data: { record, suggestions: [{ provenance: "original private context" }] }, context: { revision: 17 },
  }));
  await expect(createMentionRecordRead(fetcher, () => 1).read(base, "finance/old-record")).resolves.toEqual({
    title: record.title, description: record.description, status: record.status, version: 2,
    fields: [{ key: "f", label: "F", value: "display" }],
  });
  expect(fetcher).toHaveBeenCalledWith("/api/v1/holdings/acme/records/finance/old-record?view=mention",
    expect.objectContaining({ cache: "no-store" }));
});

it("surfaces the native authority refusal instead of displaying a record", async () => {
  for (const [status, code] of [[403, "MODULE_FORBIDDEN"], [404, "NOT_FOUND"], [409, "WORK_CONTEXT_CHANGED"]] as const) {
    const fetcher = async () => Response.json({ error: { code, message: "Original owner refusal" } }, { status });
    await expect(createMentionRecordRead(fetcher, () => 1).read(base, "finance/old-record")).rejects.toThrow("Original owner refusal");
  }
});
