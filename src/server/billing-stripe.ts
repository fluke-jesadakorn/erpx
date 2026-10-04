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
function retryRequired() {
  return new ApiError(
    503,
    "STRIPE_RETRY_REQUIRED",
    "Stripe could not complete the Sandbox operation. Retry the same command.",
  );
}
export function billingStripe(env: BetaEnvironment): BillingStripe {
  return async (path, fields, key) => {
    if (!env.STRIPE_SECRET_KEY?.startsWith("sk_test_"))
      throw new ApiError(
        503,
        "SANDBOX_REQUIRED",
        "Only Stripe Sandbox test keys are accepted.",
      );
    let r: Response;
    try {
      r = await fetch(`https://api.stripe.com/v1/${path}`, {
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
    } catch {
      // A lost response is ambiguous. Preserve the caller's original key.
      throw retryRequired();
    }
    // https://docs.stripe.com/api/errors: 401 and 403 need credential repair.
    // Never expose provider bodies or credentials in a receipt or operator UI.
    if (r.status === 401)
      throw new ApiError(
        503,
        "STRIPE_AUTHENTICATION_REQUIRED",
        "Stripe Sandbox rejected its configured key. Repair the Sandbox credential before retrying this command.",
      );
    if (r.status === 403)
      throw new ApiError(
        503,
        "STRIPE_PERMISSION_REQUIRED",
        "The configured Stripe Sandbox key cannot perform this operation. Repair its permissions before retrying this command.",
      );
    if (!r.ok) throw retryRequired();
    let raw;
    try {
      raw = await r.json();
    } catch {
      throw retryRequired();
    }
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
