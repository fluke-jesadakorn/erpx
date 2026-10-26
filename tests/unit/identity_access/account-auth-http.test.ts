import { expect, it, vi } from "vitest";
import { createAccountAuthHttpPort } from "@erpx/feature-identity_access/adapters/http/account-auth";

const user = { id: "synthetic-user", name: "Synthetic User", email: "synthetic@example.test", emailVerified: true, image: null };
const handle = "00000000-0000-4000-8000-000000000001";
const devices = { initialized: true, revision: 12, maximum: 5, accounts: [{ handle, name: user.name, email: user.email, image: null, current: true }] };
const response = (value: unknown, status = 200) => Response.json(value, { status });

it("preserves email/reset/verification call shapes with explicit credentialed HTTP requests", async () => {
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async () => response({ status: true }));
  const client = createAccountAuthHttpPort(fetcher);
  await client.signUp.email({ name: user.name, email: user.email, password: "synthetic-password", callbackURL: "/onboarding" });
  await client.requestPasswordReset({ email: user.email, redirectTo: "/reset-password" });
  await client.resetPassword({ newPassword: "synthetic-password", token: "synthetic-reset-proof" });
  await client.sendVerificationEmail({ email: user.email, callbackURL: "/" });
  await client.signOut();
  expect(fetcher.mock.calls.map(call => call[0])).toEqual(["/api/auth/sign-up/email", "/api/auth/request-password-reset", "/api/auth/reset-password", "/api/auth/send-verification-email", "/api/auth/sign-out"]);
  for (const [, init] of fetcher.mock.calls) expect(init).toMatchObject({ method: "POST", credentials: "include", cache: "no-store" });
  expect(JSON.parse(String(fetcher.mock.calls[0][1].body))).toEqual({ name: user.name, email: user.email, password: "synthetic-password", callbackURL: "/onboarding" });
});

it("accepts the Rust sign-in user contract and rejects credential-shaped response extensions", async () => {
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async () => response({ user }));
  const client = createAccountAuthHttpPort(fetcher);
  expect(await client.signIn.email({ email: user.email, password: "synthetic-password", revision: 12, rememberMe: false })).toEqual({ data: { user }, error: null });
  expect(JSON.parse(String(fetcher.mock.calls[0][1].body))).toEqual({ email: user.email, password: "synthetic-password", revision: 12, rememberMe: false });
  fetcher.mockImplementation(async () => response({ user, token: "synthetic-private-proof" }));
  expect((await client.signIn.email({ email: user.email, password: "synthetic-password" })).error?.code).toBe("ACCOUNT_RESPONSE_INVALID");
});

it("selects only opaque saved handles with the captured revision and keeps stale-selection errors", async () => {
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async () => response(devices));
  const client = createAccountAuthHttpPort(fetcher);
  expect((await client.multiSession.listDeviceSessions()).data).toEqual(devices);
  expect((await client.multiSession.setActive({ handle, revision: 12 })).data).toEqual(devices);
  expect(fetcher.mock.calls[1][0]).toBe("/api/v1/account/device-accounts/active");
  expect(JSON.parse(String(fetcher.mock.calls[1][1].body))).toEqual({ handle, revision: 12 });
  fetcher.mockImplementation(async () => response({ error: { code: "DEVICE_ACCOUNTS_CHANGED", message: "Review current accounts." } }, 409));
  expect(await client.multiSession.setActive({ handle, revision: 12 })).toEqual({ data: null, error: { code: "DEVICE_ACCOUNTS_CHANGED", message: "Review current accounts." } });
});

it("rejects lossy revisions before sending commands and rejects unsafe revisions from the server", async () => {
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async () => response(devices));
  const client = createAccountAuthHttpPort(fetcher);
  for (const revision of [-1, 1.5, Number.MAX_SAFE_INTEGER + 1, Number.POSITIVE_INFINITY]) {
    expect((await client.multiSession.setActive({ handle, revision })).error?.code).toBe("INVALID_DEVICE_SELECTION");
    expect((await client.signIn.email({ email: user.email, password: "synthetic-password", revision })).error?.code).toBe("INVALID_DEVICE_REVISION");
  }
  expect(fetcher).not.toHaveBeenCalled();
  fetcher.mockImplementation(async () => response({ ...devices, revision: Number.MAX_SAFE_INTEGER + 1 }));
  expect((await client.multiSession.listDeviceSessions()).error?.code).toBe("ACCOUNT_RESPONSE_INVALID");
});

it("rejects raw-token device rows, duplicate handles and multiple active accounts", async () => {
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async () => response({ ...devices, accounts: [{ ...devices.accounts[0], token: "synthetic-private-proof" }] }));
  const client = createAccountAuthHttpPort(fetcher);
  expect((await client.multiSession.listDeviceSessions()).error?.code).toBe("ACCOUNT_RESPONSE_INVALID");
  fetcher.mockImplementation(async () => response({ ...devices, accounts: [devices.accounts[0], devices.accounts[0]] }));
  expect((await client.multiSession.listDeviceSessions()).error?.code).toBe("ACCOUNT_RESPONSE_INVALID");
});

it("accepts nullable sessions and exact string timestamps without a bearer token", async () => {
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async () => response(null));
  const client = createAccountAuthHttpPort(fetcher);
  expect(await client.getSession()).toEqual({ data: null, error: null });
  const session = { id: "synthetic-session", userId: user.id, expiresAt: "2027-01-15T20:00:00Z", createdAt: "2027-01-15T08:00:00Z", updatedAt: "2027-01-15T08:00:00Z" };
  fetcher.mockImplementation(async () => response({ user, session }));
  expect((await client.getSession()).data).toEqual({ user, session });
  fetcher.mockImplementation(async () => response({ user, session: { ...session, token: "synthetic-private-proof" } }));
  expect((await client.getSession()).error?.code).toBe("ACCOUNT_RESPONSE_INVALID");
});

it("reports unavailable or malformed HTTP responses without fabricating authentication success", async () => {
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async (): Promise<Response> => { throw new Error("synthetic transport unavailable"); });
  const client = createAccountAuthHttpPort(fetcher);
  expect((await client.signOut()).error?.code).toBe("ACCOUNT_NETWORK_UNAVAILABLE");
  fetcher.mockImplementation(async () => new Response("Not found.", { status: 404 }));
  expect((await client.signUp.email({ name: user.name, email: user.email, password: "synthetic-password" })).data).toBeNull();
  fetcher.mockImplementation(async () => response({ status: false }));
  expect((await client.signOut()).error?.code).toBe("ACCOUNT_RESPONSE_INVALID");
});

it("uses typed verification and password-change contracts without a client authority field", async () => {
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async () => response({ user }));
  const client = createAccountAuthHttpPort(fetcher);
  expect((await client.verifyEmail({ token: "synthetic-email-proof", revision: 12, callbackURL: "/onboarding" })).data).toEqual({ user });
  expect(fetcher.mock.calls[0][0]).toBe("/api/auth/verify-email");
  expect(JSON.parse(String(fetcher.mock.calls[0][1].body))).toEqual({ token: "synthetic-email-proof", revision: 12, callbackURL: "/onboarding" });
  fetcher.mockImplementation(async () => response({ status: true }));
  expect((await client.changePassword({ currentPassword: "current-synthetic-password", newPassword: "new-synthetic-password" })).error).toBeNull();
  expect(fetcher.mock.calls[1][0]).toBe("/api/auth/change-password");
  expect(JSON.parse(String(fetcher.mock.calls[1][1].body))).toEqual({ currentPassword: "current-synthetic-password", newPassword: "new-synthetic-password" });
});
