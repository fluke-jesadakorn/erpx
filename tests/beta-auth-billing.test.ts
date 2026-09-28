import { env } from "./runtime-env";
import { applyMigrations, type TestMigration } from "./runtime-env";
import { beforeAll, it, expect, vi } from "vitest";
import {
  billingLink,
  verifyStripeSignature,
  stripeWebhook,
} from "../src/server/beta-billing";
vi.mock("../src/server/runtime", () => ({
  runtimeEnv: async () => ({
    ...env,
    ERPX_PUBLIC_BETA: "true",
    ERPX_ENVIRONMENT: "local",
    BETTER_AUTH_URL: "http://localhost:3100",
    BETTER_AUTH_SECRET: "test-secret-at-least-thirty-two-characters",
    RESEND_API_KEY: "mock-key",
    RESEND_FROM: "ERPX <test@example.test>",
  }),
}));
import { createAuth } from "../src/server/auth";
import { deliverAccountRecovery } from "../src/server/account-recovery";
beforeAll(async () => {
  await applyMigrations(
    env.CONTROL_DB,
    env.CONTROL_TEST_MIGRATIONS as TestMigration[],
  );
});
it("registers, verifies email and recovers a password through actual Better Auth handlers", async () => {
  const original = globalThis.fetch,
    emails: string[] = [];
  globalThis.fetch = vi.fn(async (_url, options) => {
    emails.push(JSON.parse(String(options?.body)).text);
    return Response.json({ id: "mail-test" });
  });
  try {
    const auth = await createAuth(),
      email = `${crypto.randomUUID()}@example.test`,
      password = "New-password-2026!";
    const post = (path: string, body: unknown) =>
      auth.handler(
        new Request(`http://localhost:3100/api/auth/${path}`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            origin: "http://localhost:3100",
          },
          body: JSON.stringify(body),
        }),
      );
    const signup = await post("sign-up/email", {
      email,
      name: "Beta owner",
      password,
      callbackURL: "/onboarding",
    });
    expect(signup.status, await signup.clone().text()).toBe(200);
    expect(emails.length).toBe(1);
    const denied = await post("sign-in/email", { email, password });
    expect(denied.status).toBe(403);
    const verification = emails[0].match(/http[^\s]+/)![0];
    const verified = await auth.handler(new Request(verification));
    expect([200, 302]).toContain(verified.status);
    expect((await post("sign-in/email", { email, password })).status).toBe(200);
    vi.stubEnv("ERPX_FRONT_PMS_ORIGIN", "http://pms.localhost:3100");
    vi.stubEnv("ERPX_FRONT_POS_ORIGIN", "http://pos.localhost:3100");
    const pmsAuth = await createAuth("http://pms.localhost:3100");
    const pmsSignIn = await pmsAuth.handler(new Request("http://pms.localhost:3100/api/auth/sign-in/email", {
      method: "POST",
      headers: { "Content-Type": "application/json", Origin: "http://pms.localhost:3100" },
      body: JSON.stringify({ email, password, callbackURL: "/" }),
    }));
    expect(pmsSignIn.status).toBe(200);
    const pmsCookie = pmsSignIn.headers.get("set-cookie") ?? "";
    expect(pmsCookie).toContain("erpx-pms.session_token");
    expect(pmsCookie).not.toContain("Domain=");
    const posAuth = await createAuth("http://pos.localhost:3100");
    const posSession = await posAuth.handler(new Request("http://pos.localhost:3100/api/auth/get-session", {
      headers: { Cookie: pmsCookie.split(";")[0] },
    }));
    expect(await posSession.json()).toBeNull();
    vi.unstubAllEnvs();
    expect(
      (
        await post("request-password-reset", {
          email,
          redirectTo: "/reset-password",
        })
      ).status,
    ).toBe(200);
    expect(emails).toHaveLength(1);
    expect(
      await deliverAccountRecovery(
        env.CONTROL_DB,
        {
          ...env,
          BETTER_AUTH_SECRET: "test-secret-at-least-thirty-two-characters",
          RESEND_API_KEY: "mock-key",
          RESEND_FROM: "ERPX <test@example.test>",
        },
      ),
    ).toBe(true);
    const link = emails.at(-1)!.match(/http[^\s]+/)![0];
    const resetRedirect = await auth.handler(new Request(link));
    const target = new URL(
        resetRedirect.headers.get("location")!,
        "http://localhost:3100",
      ),
      token = target.searchParams.get("token");
    expect(token).toBeTruthy();
    expect(
      (
        await post("reset-password", {
          token,
          newPassword: "Replaced-password-2026!",
        })
      ).status,
    ).toBe(200);
    const userId = await env.CONTROL_DB.prepare(
      'SELECT id FROM "user" WHERE email=?',
    ).bind(email).first<string>("id");
    expect(
      await env.CONTROL_DB.prepare(
        'SELECT count(*)::int count FROM session WHERE "userId"=?',
      ).bind(userId).first<number>("count"),
    ).toBe(0);
    expect(
      await env.CONTROL_DB.prepare(
        "SELECT count(*)::int count FROM account_recovery_events WHERE user_id=? AND action='completed'",
      ).bind(userId).first<number>("count"),
    ).toBe(1);
    expect((await post("sign-in/email", { email, password })).status).toBe(401);
    expect(
      (
        await post("sign-in/email", {
          email,
          password: "Replaced-password-2026!",
        })
      ).status,
    ).toBe(200);
  } finally {
    globalThis.fetch = original;
  }
});
it("disables legacy checkout and returns portal customers to Usage & billing", async () => {
  const holding = `bill-return-${crypto.randomUUID()}`;
  await env.CONTROL_DB.prepare(
    "INSERT INTO beta_billing(holding_id,customer_id,updated_at) VALUES(?,?,1)",
  ).bind(holding, `cus_${holding}`).run();
  const fetch = vi.spyOn(globalThis, "fetch").mockImplementation(async (url) =>
    Response.json({
      url: String(url).includes("billing_portal")
        ? "https://billing.stripe.com/p/session/test"
        : "https://checkout.stripe.com/c/pay/test",
    }),
  );
  try {
    const configuration = {
      ...env,
      STRIPE_SECRET_KEY: "sk_test_fixture",
      STRIPE_PRICE_ID: "price_fixture",
      BETTER_AUTH_URL: "https://erpx.example.test",
    };
    await expect(billingLink(configuration, holding, "acme", "owner@example.test", "checkout")).rejects.toMatchObject({code:"LEGACY_CHECKOUT_DISABLED"});
    await billingLink(configuration, holding, "acme", "owner@example.test", "portal");
    const portal = new URLSearchParams(String(fetch.mock.calls[0][1]?.body));
    const expected = "https://erpx.example.test/holdings/acme/settings?section=usage";
    expect(portal.get("return_url")).toBe(expected);
  } finally {
    fetch.mockRestore();
  }
});
async function signed(
  raw: string,
  secret: string,
  time = Math.floor(Date.now() / 1000),
) {
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const signature = await crypto.subtle.sign(
    "HMAC",
    key,
    new TextEncoder().encode(`${time}.${raw}`),
  );
  return `t=${time},v1=${Array.from(new Uint8Array(signature), (b) => b.toString(16).padStart(2, "0")).join("")}`;
}
it("rejects invalid signatures, stale timestamps, modified bytes and live billing events", async () => {
  const secret = "whsec_test",
    raw = JSON.stringify({
      id: "evt_test",
      type: "invoice.paid",
      created: 1,
      livemode: false,
    });
  expect(
    await verifyStripeSignature(raw, await signed(raw, secret), secret),
  ).toBe(true);
  expect(
    await verifyStripeSignature(`${raw} `, await signed(raw, secret), secret),
  ).toBe(false);
  expect(
    await verifyStripeSignature(raw, await signed(raw, secret, 1), secret),
  ).toBe(false);
  const live = raw.replace("false", "true");
  await expect(
    stripeWebhook(
      { ...env, STRIPE_WEBHOOK_SECRET: secret },
      live,
      await signed(live, secret),
    ),
  ).rejects.toMatchObject({ code: "INVALID_EVENT" });
});
it("uses a durable inbox, retries failures and ignores duplicate or older subscription snapshots", async () => {
  const secret = "whsec_test",
    holding = `bill-${crypto.randomUUID()}`,
    customer = `cus_${crypto.randomUUID()}`;
  await env.CONTROL_DB.prepare(
    "INSERT INTO beta_billing(holding_id,customer_id,updated_at) VALUES(?,?,1)",
  )
    .bind(holding, customer)
    .run();
  const configuration = { ...env, STRIPE_WEBHOOK_SECRET: secret },
    event = (
      id: string,
      created: number,
      type = "customer.subscription.updated",
    ) =>
      JSON.stringify({
        id,
        type,
        created,
        livemode: false,
        data: { object: { customer } },
      });
  const raw = event(`evt_${crypto.randomUUID()}`, 20),
    signature = await signed(raw, secret),
    retrieve = vi.fn(async () => ({
      data: [
        {
          id: `sub_${holding}`,
          created: 1,
          status: "past_due",
          livemode: false,
          current_period_end: 123,
        },
      ],
    }));
  await expect(
    stripeWebhook(configuration, raw, signature, async () => {
      throw new Error("Network failure");
    }),
  ).rejects.toThrow("Network failure");
  await stripeWebhook(configuration, raw, signature, retrieve);
  await stripeWebhook(configuration, raw, signature, retrieve);
  expect(retrieve).toHaveBeenCalledOnce();
  const older = event(`evt_${crypto.randomUUID()}`, 10);
  await stripeWebhook(
    configuration,
    older,
    await signed(older, secret),
    async () => ({
      data: [
        { id: `sub_${holding}`, created: 1, status: "active", livemode: false },
      ],
    }),
  );
  expect(
    await env.CONTROL_DB.prepare(
      "SELECT status FROM beta_billing WHERE holding_id=?",
    )
      .bind(holding)
      .first("status"),
  ).toBe("past_due");
  const cancelled = event(
    `evt_${crypto.randomUUID()}`,
    30,
    "customer.subscription.deleted",
  );
  await stripeWebhook(
    configuration,
    cancelled,
    await signed(cancelled, secret),
    async () => ({
      data: [
        {
          id: `sub_${holding}`,
          created: 1,
          status: "canceled",
          livemode: false,
        },
      ],
    }),
  );
  expect(
    await env.CONTROL_DB.prepare(
      "SELECT status FROM beta_billing WHERE holding_id=?",
    )
      .bind(holding)
      .first("status"),
  ).toBe("canceled");
});
