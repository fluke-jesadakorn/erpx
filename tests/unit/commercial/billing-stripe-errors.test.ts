import { afterEach, expect, it, vi } from "vitest";
import { billingStripe } from "../../../src/server/modules/billing/public/billing-stripe";

afterEach(() => vi.unstubAllGlobals());

it.each([
  [401, "STRIPE_AUTHENTICATION_REQUIRED"],
  [403, "STRIPE_PERMISSION_REQUIRED"],
] as const)("requires Sandbox configuration repair for provider status %i without exposing its body", async (status, code) => {
  const response = new Response("Provider details contain sk_test_private_fixture", { status });
  const fetch = vi.fn().mockResolvedValue(response);
  vi.stubGlobal("fetch", fetch);
  const error = await billingStripe({ STRIPE_SECRET_KEY: "sk_test_declared_fixture" })(
    "invoices", { customer: "cus_declared_fixture" }, "original-review-command",
  ).catch((error: unknown) => error);
  expect(error).toMatchObject({ status: 503, code });
  expect(String(error)).not.toContain("sk_test_");
  expect(response.bodyUsed).toBe(false);
  expect(fetch).toHaveBeenCalledOnce();
  expect(fetch.mock.calls[0][1].headers["Idempotency-Key"]).toBe("original-review-command");
});

it("keeps transient and lost responses on the original Sandbox operation key", async () => {
  const fetch = vi.fn()
    .mockResolvedValueOnce(new Response("Rate limited", { status: 429 }))
    .mockResolvedValueOnce(new Response("Provider unavailable", { status: 503 }))
    .mockRejectedValueOnce(new Error("Private transport details sk_test_private_fixture"))
    .mockResolvedValueOnce(Response.json({ id: "in_replayed_fixture", livemode: false }));
  vi.stubGlobal("fetch", fetch);
  const stripe = billingStripe({ STRIPE_SECRET_KEY: "sk_test_declared_fixture" });
  for (let attempt = 0; attempt < 3; attempt++) {
    const error = await stripe("invoices", { customer: "cus_declared_fixture" }, "original-review-command")
      .catch((error: unknown) => error);
    expect(error).toMatchObject({ status: 503, code: "STRIPE_RETRY_REQUIRED" });
    expect(String(error)).not.toContain("sk_test_");
  }
  expect(await stripe("invoices", { customer: "cus_declared_fixture" }, "original-review-command"))
    .toMatchObject({ id: "in_replayed_fixture", livemode: false });
  expect(fetch).toHaveBeenCalledTimes(4);
  for (const [url, options] of fetch.mock.calls) {
    expect(url).toBe("https://api.stripe.com/v1/invoices");
    expect(options.method).toBe("POST");
    expect(options.headers["Idempotency-Key"]).toBe("original-review-command");
    expect(String(options.body)).toBe("customer=cus_declared_fixture");
  }
});
