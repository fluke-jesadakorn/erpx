import { z } from "zod";
import { ApiError } from "./http";
import type { BetaEnvironment } from "./beta-config";
const object = z
  .object({ id: z.string(), livemode: z.boolean().optional() })
  .passthrough();
export type StripeObject = z.infer<typeof object>;
export type BillingStripe = (
  path: string,
  fields?: Record<string, string>,
  key?: string,
) => Promise<StripeObject>;
export function billingStripe(env: BetaEnvironment): BillingStripe {
  return async (path, fields, key) => {
    if (!env.STRIPE_SECRET_KEY?.startsWith("sk_test_"))
      throw new ApiError(
        503,
        "SANDBOX_REQUIRED",
        "Only Stripe Sandbox test keys are accepted.",
      );
    const r = await fetch(`https://api.stripe.com/v1/${path}`, {
      method: fields ? "POST" : "GET",
      headers: {
        Authorization: `Bearer ${env.STRIPE_SECRET_KEY}`,
        "Stripe-Version": "2025-02-24.acacia",
        ...(fields
          ? { "Content-Type": "application/x-www-form-urlencoded" }
          : {}),
        ...(key ? { "Idempotency-Key": key } : {}),
      },
      body: fields ? new URLSearchParams(fields) : undefined,
      signal: AbortSignal.timeout(15000),
    });
    const raw = await r.json();
    if (!r.ok)
      throw new ApiError(
        503,
        "STRIPE_RETRY_REQUIRED",
        "Stripe could not complete the Sandbox operation. The same command will be retried.",
      );
    // List/portal objects do not carry livemode. Every monetary object must be explicitly test.
    if (
      raw.livemode === true ||
      (Array.isArray(raw.data) &&
        raw.data.some((v: { livemode?: boolean }) => v.livemode === true))
    )
      throw new Error("LIVE_STRIPE_OBJECT_REJECTED");
    return object.parse({ ...raw, id: raw.id ?? "list" });
  };
}
export function stripeUrl(value: unknown, hosts: string[]) {
  if (typeof value !== "string") return null;
  const u = new URL(value);
  if (u.protocol !== "https:" || !hosts.includes(u.hostname))
    throw new Error("INVALID_STRIPE_URL");
  return value;
}
export function stripeList(value: StripeObject): StripeObject[] {
  return z.array(object).parse(value.data ?? []);
}
export function testObject(value: StripeObject) {
  if (value.livemode !== false) throw new Error("TEST_STRIPE_OBJECT_REQUIRED");
  return value;
}
