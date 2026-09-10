import type { SqlDatabase } from "./runtime/types";
import { z } from "zod";
const stripeObject = z.object({
  id: z.string().optional(),
  url: z.string().optional(),
  livemode: z.boolean().optional(),
  data: z
    .array(
      z.object({
        id: z.string(),
        livemode: z.boolean(),
        created: z.number(),
        status: z.string(),
        current_period_end: z.number().nullable().optional(),
      }),
    )
    .optional(),
});
import { ApiError } from "./http";
import type { BetaEnvironment } from "./beta-config";
type Env = BetaEnvironment & {
  CONTROL_DB: SqlDatabase;
  BETTER_AUTH_URL: string;
};
export async function stripeRequest(
  env: BetaEnvironment,
  path: string,
  fields?: Record<string, string>,
  key?: string,
) {
  if (!env.STRIPE_SECRET_KEY?.startsWith("sk_test_"))
    throw new ApiError(
      503,
      "BILLING_NOT_CONFIGURED",
      "Test billing is not configured. Beta remains free.",
    );
  const response = await fetch(`https://api.stripe.com/v1/${path}`, {
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
  if (!response.ok)
    throw new ApiError(
      503,
      "BILLING_UNAVAILABLE",
      "Stripe test billing is temporarily unavailable.",
    );
  return stripeObject.parse(await response.json());
}
export async function billingLink(
  env: Env,
  holdingId: string,
  slug: string,
  email: string,
  action: "checkout" | "portal",
) {
  let row = await env.CONTROL_DB.prepare(
    "SELECT customer_id FROM beta_billing WHERE holding_id=?",
  )
    .bind(holdingId)
    .first<{ customer_id: string | null }>();
  if (!row)
    throw new ApiError(404, "BILLING_UNAVAILABLE", "Beta company not found.");
  if (!row.customer_id) {
    const customer = await stripeRequest(
      env,
      "customers",
      { email, "metadata[holding_id]": holdingId },
      `erpx-beta-customer-${holdingId}`,
    );
    if (typeof customer.id !== "string" || customer.livemode)
      throw new Error("INVALID_STRIPE_CUSTOMER");
    await env.CONTROL_DB.prepare(
      "UPDATE beta_billing SET customer_id=?,updated_at=? WHERE holding_id=? AND customer_id IS NULL",
    )
      .bind(customer.id, Date.now(), holdingId)
      .run();
    row = await env.CONTROL_DB.prepare(
      "SELECT customer_id FROM beta_billing WHERE holding_id=?",
    )
      .bind(holdingId)
      .first<{ customer_id: string }>();
  }
  const returnUrl = new URL(
    `/holdings/${slug}/settings`,
    env.BETTER_AUTH_URL,
  ).toString();
  if (action === "checkout" && !env.STRIPE_PRICE_ID)
    throw new ApiError(
      503,
      "PRICE_NOT_CONFIGURED",
      "Configure a test price before testing checkout.",
    );
  const result = await stripeRequest(
    env,
    action === "portal" ? "billing_portal/sessions" : "checkout/sessions",
    action === "portal"
      ? { customer: row!.customer_id!, return_url: returnUrl }
      : {
          customer: row!.customer_id!,
          mode: "subscription",
          "line_items[0][price]": env.STRIPE_PRICE_ID!,
          "line_items[0][quantity]": "1",
          success_url: returnUrl,
          cancel_url: returnUrl,
          "subscription_data[metadata][holding_id]": holdingId,
        },
  );
  if (
    typeof result.url !== "string" ||
    !["checkout.stripe.com", "billing.stripe.com"].includes(
      new URL(result.url).hostname,
    )
  )
    throw new Error("INVALID_STRIPE_URL");
  return { url: result.url, testMode: true };
}
export async function verifyStripeSignature(
  raw: string,
  header: string | null,
  secret: string,
  now = Date.now(),
) {
  const parts = (header ?? "").split(","),
    time = parts.find((p) => p.startsWith("t="))?.slice(2),
    signatures = parts
      .filter((p) => p.startsWith("v1="))
      .map((p) => p.slice(3));
  if (!time || !/^\d+$/.test(time) || Math.abs(now / 1000 - Number(time)) > 300)
    return false;
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["verify"],
  );
  for (const signature of signatures) {
    if (!/^[a-f0-9]{64}$/.test(signature)) continue;
    const bytes = Uint8Array.from(signature.match(/../g)!, (x) =>
      parseInt(x, 16),
    );
    if (
      await crypto.subtle.verify(
        "HMAC",
        key,
        bytes,
        new TextEncoder().encode(`${time}.${raw}`),
      )
    )
      return true;
  }
  return false;
}
export async function stripeWebhook(
  env: Env,
  raw: string,
  signature: string | null,
  retrieve = stripeRequest,
) {
  if (
    !env.STRIPE_WEBHOOK_SECRET ||
    !(await verifyStripeSignature(raw, signature, env.STRIPE_WEBHOOK_SECRET))
  )
    throw new ApiError(400, "INVALID_SIGNATURE", "Invalid Stripe signature.");
  const event = JSON.parse(raw);
  if (
    event.livemode !== false ||
    typeof event.id !== "string" ||
    typeof event.type !== "string" ||
    !Number.isSafeInteger(event.created)
  )
    throw new ApiError(
      400,
      "INVALID_EVENT",
      "Only Stripe test events are accepted.",
    );
  const db = env.CONTROL_DB;
  await db
    .prepare(
      "INSERT INTO stripe_inbox(id,event_type,created_at,payload_json) VALUES(?,?,?,?) ON CONFLICT DO NOTHING",
    )
    .bind(event.id, event.type, event.created, raw)
    .run();
  const inbox = await db
    .prepare("SELECT state FROM stripe_inbox WHERE id=?")
    .bind(event.id)
    .first<{ state: string }>();
  if (inbox?.state !== "pending") return { received: true };
  const relevant =
      /^(customer\.subscription\.|invoice\.(paid|payment_failed|payment_action_required)|checkout\.session\.completed)/.test(
        event.type,
      ),
    customer = event.data?.object?.customer;
  const row =
    typeof customer === "string"
      ? await db
          .prepare("SELECT holding_id FROM beta_billing WHERE customer_id=?")
          .bind(customer)
          .first<{ holding_id: string }>()
      : null;
  if (!relevant || !row) {
    await db
      .prepare(
        "UPDATE stripe_inbox SET state='ignored',applied_at=? WHERE id=?",
      )
      .bind(Date.now(), event.id)
      .run();
    return { received: true };
  }
  // Query current Stripe state; an older delivery must never restore an obsolete subscription snapshot.
  const subscriptions = await retrieve(
    env,
    `subscriptions?customer=${encodeURIComponent(customer)}&status=all&limit=10`,
  );
  const list = Array.isArray(subscriptions.data) ? subscriptions.data : [];
  if (list.some((s) => s.livemode !== false))
    throw new Error("LIVE_SUBSCRIPTION_REJECTED");
  const current = list.sort((a, b) => b.created - a.created)[0];
  await db.batch([
    db
      .prepare(
        "UPDATE beta_billing SET subscription_id=?,status=?,current_period_end=?,last_event_at=?,updated_at=? WHERE holding_id=? AND last_event_at<=?",
      )
      .bind(
        current?.id ?? null,
        current?.status ?? "free_beta",
        current?.current_period_end ?? null,
        event.created,
        Date.now(),
        row.holding_id,
        event.created,
      ),
    db
      .prepare(
        "UPDATE stripe_inbox SET state='applied',applied_at=? WHERE id=?",
      )
      .bind(Date.now(), event.id),
  ]);
  return { received: true };
}
