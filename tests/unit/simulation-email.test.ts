import { describe, expect, it } from "vitest";
import { simulationBookingEmailProvider } from "../../src/server/modules/pms/adapters/simulation-email";

describe("isolated booking email provider", () => {
  const cohort = { ERPX_ENVIRONMENT: "local", ERPX_EMAIL_ADAPTER: "sandbox", ERPX_PUBLIC_ORIGIN: "http://127.0.0.1:3102", ERPX_DATA_DIR: "/repo/.runtime/browser/abcdef123456" };
  it("cannot replace providers for development, production or an unowned file store", () => {
    for (const overrides of [
      { ERPX_PUBLIC_ORIGIN: "http://127.0.0.1:3100" },
      { ERPX_PUBLIC_ORIGIN: "https://console-dev-erpx.jesadakorn.com" },
      { ERPX_ENVIRONMENT: "production" },
      { ERPX_EMAIL_ADAPTER: "live" },
      { ERPX_DATA_DIR: "/repo/private-files" },
      { ERPX_DATA_DIR: "/repo/.runtime/browser/abcdef123456/nested" },
    ]) expect(simulationBookingEmailProvider({ ...cohort, ...overrides })).toBeUndefined();
  });
  it("returns the same provider receipt for a lost-response replay without network access", async () => {
    const provider = simulationBookingEmailProvider(cohort)!;
    const request = { method: "POST", headers: { "Idempotency-Key": "pms-booking-delivery-123" } };
    const first = await (await provider("https://api.resend.com/emails", request)).json();
    expect(await (await provider("https://api.resend.com/emails", request)).json()).toEqual(first);
    expect(first).toMatchObject({ id: "sandbox-pms-booking-delivery-123" });
  });
  it("fails unexpected destinations and commands instead of sending anything", async () => {
    const provider = simulationBookingEmailProvider(cohort)!;
    await expect(provider("https://external.invalid", { method: "POST" })).rejects.toThrow("UNEXPECTED_SIMULATION_BOOKING_MAIL_REQUEST");
    await expect(provider("https://api.resend.com/emails", { method: "POST", headers: { "Idempotency-Key": "ordinary-email" } })).rejects.toThrow("UNEXPECTED_SIMULATION_BOOKING_MAIL_REQUEST");
  });
});
