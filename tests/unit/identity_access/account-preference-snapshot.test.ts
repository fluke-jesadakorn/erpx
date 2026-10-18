import { beforeEach, expect, it, vi } from "vitest";
vi.mock("server-only", () => ({}));
const mocks = vi.hoisted(() => ({ session: vi.fn(), profile: vi.fn() }));
vi.mock("@/server/modules/identity_access/adapters/authorization", () => ({ currentSession: mocks.session }));
vi.mock("@/server/modules/identity_access/adapters/account-profile", () => ({ currentAccountProfile: mocks.profile }));
import { accountPreferenceSnapshot, preferredAccountAppearance, preferredAccountLanguage } from "@/server/modules/identity_access/public/account-preference-snapshot";

beforeEach(() => { mocks.session.mockReset(); mocks.profile.mockReset(); });
it("projects only the current account's opaque preference seed, without identity, session, roles or private profile fields", async () => {
  mocks.session.mockResolvedValue({ session: { user: { id: "private-user", email: "private@example.test" }, session: { token: "private-token" } } });
  mocks.profile.mockResolvedValue({ accountKey: "unused", profile: { name: "Private name", email: "private@example.test",
    preferences: { language: "th", appearance: "dark", version: 4 } } });
  const snapshot = await accountPreferenceSnapshot();
  expect(snapshot).toEqual({ key: expect.stringMatching(/^[a-f0-9]{64}$/), preference: { language: "th", appearance: "dark", version: 4 } });
  expect(JSON.stringify(snapshot)).not.toMatch(/private-user|Private name|private@example|private-token|roles/);
  expect(await preferredAccountAppearance("light")).toBe("dark"); expect(await preferredAccountLanguage("en")).toBe("th");
});
it("keeps existing host defaults when the account has never saved preferences", async () => {
  mocks.session.mockResolvedValue({ session: { user: { id: "one" } } });
  mocks.profile.mockResolvedValue({ profile: { preferences: { language: "en", appearance: "system", version: 0 } } });
  expect(await preferredAccountAppearance("dark")).toBe("dark"); expect(await preferredAccountLanguage("th")).toBe("th");
});
it("retains the same account during a preference outage and uses host fallbacks", async () => {
  mocks.session.mockResolvedValue({ session: { user: { id: "one" } } });
  mocks.profile.mockResolvedValueOnce({ profile: { preferences: { language: "th", appearance: "dark", version: 2 } } });
  const initial = await accountPreferenceSnapshot();
  mocks.profile.mockRejectedValue(new Error("Optional preference read unavailable."));
  expect(await accountPreferenceSnapshot()).toEqual({ key: initial!.key, preference: undefined });
  expect(await preferredAccountAppearance("light")).toBe("light");
});
it("does not query profile data for anonymous, revoked or unknown authentication", async () => {
  mocks.session.mockRejectedValueOnce(new Error("UNAUTHENTICATED")).mockRejectedValueOnce(new Error("Revoked"));
  expect(await accountPreferenceSnapshot()).toEqual({ key: "anonymous", preference: { language: "en", appearance: "system", version: 0 } });
  expect(await accountPreferenceSnapshot()).toBeNull(); expect(mocks.profile).not.toHaveBeenCalled();
});
it("isolates account switching even when the previous account had a newer preference revision", async () => {
  mocks.session.mockResolvedValueOnce({ session: { user: { id: "one" } } }).mockResolvedValueOnce({ session: { user: { id: "two" } } });
  mocks.profile.mockResolvedValueOnce({ profile: { preferences: { language: "th", appearance: "dark", version: 8 } } })
    .mockResolvedValueOnce({ profile: { preferences: { language: "en", appearance: "system", version: 0 } } });
  const first = await accountPreferenceSnapshot(), second = await accountPreferenceSnapshot();
  expect(second!.key).not.toBe(first!.key); expect(second!.preference!.version).toBe(0);
});
