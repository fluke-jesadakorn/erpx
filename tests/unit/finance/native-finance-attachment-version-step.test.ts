import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { nativeFinanceAttachmentVersionStep } from "../../../packages/server-finance/src/adapters/native-finance-owner-write";

const KEY = "ERPX_NATIVE_FINANCE_OWNER_WRITE_CONFIG";
const original = process.env[KEY];

const input = {
  companyId: "company-1",
  targetType: "finance_journal",
  targetId: "journal-1",
  expectedVersion: 2,
  now: 77,
  userId: "user-1",
  assignmentId: "assignment-1",
  role: "finance.manager",
  scopeKind: "company",
  key: "command-1",
} as const;

describe("native Finance attachment version step port", () => {
  beforeEach(() => {
    delete process.env[KEY];
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    if (original === undefined) delete process.env[KEY];
    else process.env[KEY] = original;
  });

  it("is a SQL-free native descriptor that fails closed without a transport", async () => {
    const step = nativeFinanceAttachmentVersionStep(input);
    expect(step.__nativeWrite).toBe(true);
    expect((step as unknown as {query?:unknown}).query).toBeUndefined();
    await expect(step.run()).rejects.toThrow("The native Finance owner write is unavailable.");
  });

  it("posts typed actor facts to the mounted step route and accepts the bounded reply", async () => {
    process.env[KEY] = JSON.stringify({
      privateBaseUrl: "http://127.0.0.1:4100",
      timeoutMs: 1000,
      holdings: [{ origin: "https://acme.jesadakorn.com", holdingSlug: "acme", holdingId: "h001" }],
    });
    const fetchMock = vi.fn(async () => new Response(
      JSON.stringify({ data: {
        ok: true,
        targetType: input.targetType,
        targetId: input.targetId,
        version: input.expectedVersion + 1,
        changed: true,
        sequencing: { sql: "SELECT 1", values: [] },
      } }),
      { status: 200, headers: { "content-type": "application/json" } },
    ));
    vi.stubGlobal("fetch", fetchMock);
    const step = nativeFinanceAttachmentVersionStep(input);
    const result = await step.run();
    expect(result.success).toBe(true);
    expect(result.results).toEqual([]);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0]! as unknown as [URL, RequestInit];
    expect(String(url)).toBe("http://127.0.0.1:4100/api/v1/companies/company-1/finance/attachment-version/step");
    expect((init.headers as Record<string, string>)["idempotency-key"]).toMatch(/^fow-finance_attachment_step-[0-9a-f]{8}$/);
    expect(JSON.parse(init.body as string)).toEqual({
      targetType: input.targetType,
      targetId: input.targetId,
      expectedVersion: input.expectedVersion,
      userId: input.userId,
      assignmentId: input.assignmentId,
      role: input.role,
      scopeKind: input.scopeKind,
      now: input.now,
    });
  });

  it("rejects an unbounded reply without invoking a TypeScript fallback", async () => {
    process.env[KEY] = JSON.stringify({
      privateBaseUrl: "http://127.0.0.1:4100",
      timeoutMs: 1000,
      holdings: [{ origin: "https://acme.jesadakorn.com", holdingSlug: "acme", holdingId: "h001" }],
    });
    vi.stubGlobal("fetch", vi.fn(async () => new Response(
      JSON.stringify({ data: { ok: false } }),
      { status: 200, headers: { "content-type": "application/json" } },
    )));
    await expect(nativeFinanceAttachmentVersionStep(input).run()).rejects.toThrow(
      "The native Finance owner write is unavailable.",
    );
  });
});
