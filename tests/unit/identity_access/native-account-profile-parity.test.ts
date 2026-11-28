import { beforeEach, expect, it, vi } from "vitest";
vi.mock("server-only", () => ({}));

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

import { nativeAccountProfile, parseAccountProfile } from "@erpx/identity-access-server/public/account-profile-native";

const config = JSON.stringify({
  privateBaseUrl: "http://127.0.0.1:43111",
  host: "acme.example.test",
  holdingSlug: "acme",
  timeoutMs: 2000,
});

function reply(body: unknown, status = 200) {
  return { ok: status >= 200 && status < 300, status, json: async () => body } as Response;
}

beforeEach(() => {
  fetchMock.mockReset();
  process.env.ERPX_NATIVE_ACCOUNT_PROFILE_CONFIG = config;
});

it("validates the exact Rust payload projection", () => {
  expect(parseAccountProfile({
    name: "Ada",
    email: "ada@example.test",
    preferences: { language: "th", appearance: "dark", version: 7 },
  })).toEqual({
    name: "Ada",
    email: "ada@example.test",
    preferences: { language: "th", appearance: "dark", version: 7 },
  });
  expect(() => parseAccountProfile({
    name: "Ada", email: "ada@example.test",
    preferences: { language: "fr", appearance: "dark", version: 7 },
  })).toThrow();
});

it("reads the profile from the native owner with exact host and context headers", async () => {
  fetchMock.mockResolvedValueOnce(reply({ data: {
    name: "Ada", email: "ada@example.test",
    preferences: { language: "en", appearance: "system", version: 0 },
  } }));
  const profile = await nativeAccountProfile("user-1");
  expect(profile.preferences).toEqual({ language: "en", appearance: "system", version: 0 });
  const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
  expect(url).toBe("http://127.0.0.1:43111/api/v1/holdings/acme/account/profile");
  expect(init.method).toBe("GET");
  const headers = new Headers(init.headers);
  expect(headers.get("host")).toBe("acme.example.test");
  expect(headers.get("x-erpx-context-user")).toBe("user-1");
});

it("fails closed when the native runtime is absent or rejects", async () => {
  delete process.env.ERPX_NATIVE_ACCOUNT_PROFILE_CONFIG;
  await expect(nativeAccountProfile("user-1")).rejects.toMatchObject({ code: "ACCOUNT_PROFILE_UNAVAILABLE" });
  process.env.ERPX_NATIVE_ACCOUNT_PROFILE_CONFIG = config;
  fetchMock.mockResolvedValueOnce(reply({ error: { code: "SIGN_IN_REQUIRED", message: "Sign in." } }, 401));
  await expect(nativeAccountProfile("user-1")).rejects.toMatchObject({ code: "SIGN_IN_REQUIRED", status: 401 });
});
