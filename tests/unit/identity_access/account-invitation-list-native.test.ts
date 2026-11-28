import { afterEach, beforeEach, expect, it, vi } from "vitest";
vi.mock("server-only", () => ({}));
import { accountInvitationList, setInvitationListForTest } from "@erpx/identity-access-server/public/account-invitation-list";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";

const actor = {
  userId: "user-1",
  role: "holding_owner",
  holdingId: "holding-1",
  companyId: "company-1",
  branchId: "branch-1",
} as unknown as WorkContext;

const item = {
  id: "inv-1",
  person_id: "person-1",
  email: "ada@example.test",
  status: "pending",
  expires_at: 1_800_000_000_000,
  delivery_state: "sent",
  attempts: 2,
  error: null,
  sent_at: 1_790_000_000_000,
};

const config = () => JSON.stringify({
  privateBaseUrl: "http://127.0.0.1:44999",
  host: "acme.example.test",
  holdingSlug: "acme",
  origin: "https://acme.example.test",
  timeoutMs: 1000,
});

afterEach(() => {
  setInvitationListForTest(undefined);
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
});

it("keeps the Holding-owner-only scope gate", async () => {
  const list = vi.fn().mockResolvedValue([item]);
  setInvitationListForTest(list);
  expect(await accountInvitationList({ ...actor, role: "employee" } as unknown as WorkContext)).toBeNull();
  expect(list).not.toHaveBeenCalled();
  expect(await accountInvitationList(actor)).toEqual([item]);
  expect(list).toHaveBeenCalledExactlyOnceWith({
    holdingId: "holding-1", companyId: "company-1", branchId: "branch-1",
  });
});

it("reads the native GET with the isolated scope and no SQL fallback", async () => {
  vi.stubEnv("ERPX_NATIVE_ACCOUNT_INVITATION_LIST_CONFIG", config());
  const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify({ invitations: [item] }), { status: 200 }));
  vi.stubGlobal("fetch", fetchMock);
  expect(await accountInvitationList(actor)).toEqual([item]);
  expect(fetchMock).toHaveBeenCalledExactlyOnceWith(
    "http://127.0.0.1:44999/api/v1/holdings/acme/account/invitations",
    expect.objectContaining({ method: "GET", cache: "no-store" }),
  );
  const headers = fetchMock.mock.calls[0][1].headers as Headers;
  expect(headers.get("host")).toBe("acme.example.test");
  expect(headers.get("x-erpx-context-holding")).toBe("holding-1");
  expect(headers.get("x-erpx-context-company")).toBe("company-1");
  expect(headers.get("x-erpx-context-branch")).toBe("branch-1");
});

it("fails closed when the native reply is malformed", async () => {
  vi.stubEnv("ERPX_NATIVE_ACCOUNT_INVITATION_LIST_CONFIG", config());
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(JSON.stringify({ invitations: [{ id: 1 }] }), { status: 200 })));
  await expect(accountInvitationList(actor)).rejects.toMatchObject({ status: 503 });
});
