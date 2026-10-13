import { beforeEach, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
  calls: [] as string[],
  withdraw: vi.fn(),
  status: vi.fn(),
  write: { name: "holding-write" },
}));

vi.mock("../../../src/server/modules/files/public/attachments/service", () => ({
  withdrawAttachmentLink: state.withdraw,
  attachmentLinkStatus: state.status,
}));

vi.mock("../../../src/server/modules/identity_access/public/authorization", () => ({
  requireWorkContext: async () => ({ holdingId: "h001", companyId: "acme-co", userId: "requester" }),
}));

vi.mock("../../../src/server/modules/manage_erpx/public/public-origin", () => ({
  requestPublicOrigin: () => "https://dev-erpx.example.test",
}));

vi.mock("../../../src/server/modules/runtime/public/runtime", () => ({
  holdingResources: async () => ({ write: state.write }),
}));

import { POST } from "../../../src/app/api/v1/holdings/[holdingSlug]/attachment-links/[linkId]/withdraw/route";

beforeEach(() => {
  state.calls = [];
  state.withdraw.mockReset().mockImplementation(async () => {
    state.calls.push("withdraw");
    return { id: "link-1", state: "withdrawn" };
  });
  state.status.mockReset().mockImplementation(async () => {
    state.calls.push("status");
    return {
      linkId: "link-1",
      setId: "set-1",
      sourceId: "source-1",
      sourceState: "available",
      linkState: "withdrawn",
      scan: { attempts: 1, lastResult: "clean", lastError: null, scannedAt: 10 },
      previewAvailable: true,
      bindable: false,
      expiresAt: 20,
      boundAt: null,
      withdrawnAt: 30,
    };
  });
});

it("returns the post-withdraw status contract consumed by AttachmentPanel", async () => {
  const response = await POST(
    new Request("https://dev-erpx.example.test/api/v1/holdings/acme/attachment-links/link-1/withdraw", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": "withdraw-001",
        Origin: "https://dev-erpx.example.test",
      },
      body: JSON.stringify({ reason: "Replace the incorrect photo" }),
    }),
    { params: Promise.resolve({ holdingSlug: "acme", linkId: "link-1" }) },
  );

  expect(response.status).toBe(200);
  expect(response.headers.get("cache-control")).toBe("private, no-store");
  expect(state.calls).toEqual(["withdraw", "status"]);
  expect(state.withdraw).toHaveBeenCalledWith(
    state.write,
    expect.objectContaining({ holdingId: "h001", companyId: "acme-co", userId: "requester" }),
    "link-1",
    "Replace the incorrect photo",
    "withdraw-001",
  );
  expect(state.status).toHaveBeenCalledWith(
    state.write,
    expect.objectContaining({ holdingId: "h001" }),
    "link-1",
  );
  await expect(response.json()).resolves.toMatchObject({
    data: { linkId: "link-1", sourceState: "available", linkState: "withdrawn", withdrawnAt: 30 },
  });
});
