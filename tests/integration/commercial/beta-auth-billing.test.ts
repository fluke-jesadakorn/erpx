import {measureReads} from "@erpx/server-runtime/adapters/runtime/read-metrics";
import { env } from "../../support/runtime-env";
import { applyMigrations, type TestMigration } from "../../support/runtime-env";
import { beforeAll, it, expect, vi } from "vitest";
import {
  billingLink,
  verifyStripeSignature,
  stripeWebhook,
} from "@erpx/server-billing/public/beta-billing";
const authRuntime = {
    ...env,
    ERPX_PUBLIC_BETA: "true",
    ERPX_ENVIRONMENT: "local",
    BETTER_AUTH_URL: "http://localhost:3100",
    BETTER_AUTH_SECRET: "test-secret-at-least-thirty-two-characters",
    RESEND_API_KEY: "mock-key",
    RESEND_FROM: "ERPX <test@example.test>",
};
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({ runtimeEnv: async () => authRuntime }));
import { createAuth } from "@erpx/server-identity_access/public/auth";
import { deliverAccountRecovery } from "@erpx/server-identity_access/public/account-recovery";
beforeAll(async () => {
  await applyMigrations(
    env.CONTROL_DB,
    env.CONTROL_TEST_MIGRATIONS as TestMigration[],
  );
});
it("registers, verifies email and recovers a password through actual Better Auth handlers", async () => {
  const original = globalThis.fetch,
    emails: string[] = [];
  vi.stubGlobal("fetch", vi.fn(async (_url: string | URL | Request, options?: RequestInit) => {
    emails.push(JSON.parse(String(options?.body)).text);
    return Response.json({ id: "mail-test" });
  }));
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
    const sessionHeaders=new Headers({Cookie:pmsCookie.split(';')[0]});
    const firstSession=await measureReads(()=>pmsAuth.api.getSession({headers:sessionHeaders}));
    expect(firstSession.metrics.queries).toBe(1);
    expect(firstSession.result?.user).toMatchObject({email,emailVerified:true,name:'Beta owner'});
    expect(firstSession.result?.user.createdAt).toBeInstanceOf(Date);
    expect(firstSession.result?.session.expiresAt).toBeInstanceOf(Date);
    const factories = await Promise.all(Array.from({length:8}, () => createAuth("http://pms.localhost:3100")));
    const concurrentSessions = await Promise.all(factories.map(factory => measureReads(() => factory.api.getSession({headers:sessionHeaders}))));
    expect(concurrentSessions.map(read => read.metrics.queries)).toEqual(Array(8).fill(1));
    expect(concurrentSessions.map(read => read.result?.user.id)).toEqual(Array(8).fill(firstSession.result!.user.id));
    // Reuse initialization resources while the actual session reads remain fresh.
    // Compare a count, never serialize an auth factory containing private config.
    expect(new Set(factories).size).toBe(1);
    await env.CONTROL_DB.prepare('UPDATE "user" SET name=?,"emailVerified"=0 WHERE id=?').bind('Fresh mock profile',firstSession.result!.user.id).run();
    const updatedSession=await measureReads(async()=>(await createAuth("http://pms.localhost:3100")).api.getSession({headers:sessionHeaders}));
    expect(updatedSession.metrics.queries).toBe(1);expect(updatedSession.result?.user.name).toBe('Fresh mock profile');
    expect(updatedSession.result?.user.emailVerified).toBe(false);
    await env.CONTROL_DB.prepare('UPDATE "user" SET "emailVerified"=1 WHERE id=?').bind(firstSession.result!.user.id).run();
    const posAuth = await createAuth("http://pos.localhost:3100");
    const posSession = await posAuth.handler(new Request("http://pos.localhost:3100/api/auth/get-session", {
      headers: { Cookie: pmsCookie.split(";")[0] },
    }));
    expect(await posSession.json()).toBeNull();
    expect((await pmsAuth.api.getSession({headers:sessionHeaders}))?.session.id).toBe(firstSession.result!.session.id);
    await env.CONTROL_DB.prepare('DELETE FROM session WHERE id=?').bind(firstSession.result!.session.id).run();
    expect(await pmsAuth.api.getSession({headers:sessionHeaders})).toBeNull();
    const expiringSignIn=await pmsAuth.handler(new Request('http://pms.localhost:3100/api/auth/sign-in/email',{
      method:'POST',headers:{'Content-Type':'application/json',Origin:'http://pms.localhost:3100'},body:JSON.stringify({email,password})
    }));
    expect(expiringSignIn.status).toBe(200);
    const expiringHeaders=new Headers({Cookie:(expiringSignIn.headers.get('set-cookie')??'').split(';')[0]});
    const expiringSession=await pmsAuth.api.getSession({headers:expiringHeaders});expect(expiringSession).not.toBeNull();
    await env.CONTROL_DB.prepare('UPDATE session SET "expiresAt"=? WHERE id=?').bind(new Date(Date.now()-1000).toISOString(),expiringSession!.session.id).run();
    expect(await pmsAuth.api.getSession({headers:expiringHeaders})).toBeNull();

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
    vi.unstubAllEnvs();
    globalThis.fetch = original;
  }
});
it("shares initialization while concurrent accounts, secret rotation and a different Control database retain fresh session boundaries", async () => {
  const originalDatabase=authRuntime.CONTROL_DB, originalSecret=authRuntime.BETTER_AUTH_SECRET;
  vi.stubEnv('ERPX_FRONT_PMS_ORIGIN','http://pms.localhost:3100');
  vi.stubEnv('ERPX_FRONT_POS_ORIGIN','http://pos.localhost:3100');
  try {
    const {hashPassword}=await import('better-auth/crypto');
    const password='Fictional-factory-password-2026!', hash=await hashPassword(password), clock=Date.now();
    const users=Array.from({length:2},(_,i)=>({id:crypto.randomUUID(),email:`${crypto.randomUUID()}@example.test`,name:`Fictional factory staff ${i}`}));
    for(const user of users)await env.CONTROL_DB.batch([
      env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?)').bind(user.id,user.name,user.email,clock,clock),
      env.CONTROL_DB.prepare('INSERT INTO account(id,"accountId","providerId",issuer,"userId",password,"createdAt","updatedAt") VALUES(?,?,\'credential\',\'local:credential\',?,?,?,?)').bind(crypto.randomUUID(),user.id,user.id,hash,clock,clock),
    ]);
    const factory=await createAuth('http://pms.localhost:3100'), cookies:Headers[]=[];
    for(const user of users){
      const response=await factory.handler(new Request('http://pms.localhost:3100/api/auth/sign-in/email',{method:'POST',headers:{'Content-Type':'application/json',Origin:'http://pms.localhost:3100'},body:JSON.stringify({email:user.email,password})}));
      expect(response.status).toBe(200);
      cookies.push(new Headers({Cookie:(response.headers.get('set-cookie')??'').split(';')[0]}));
    }
    const factories=await Promise.all(Array.from({length:12},()=>createAuth('http://pms.localhost:3100')));
    const sessions=await Promise.all(factories.map((auth,i)=>measureReads(()=>auth.api.getSession({headers:cookies[i%2]}))));
    expect(new Set(factories).size).toBe(1);
    expect(sessions.map(read=>read.metrics.queries)).toEqual(Array(12).fill(1));
    expect(sessions.map(read=>read.result?.user.id)).toEqual(Array.from({length:12},(_,i)=>users[i%2].id));
    const fresh=async(index:number)=>measureReads(async()=>(await createAuth('http://pms.localhost:3100')).api.getSession({headers:cookies[index]}));
    await env.CONTROL_DB.prepare('UPDATE "user" SET name=? WHERE id=?').bind('Updated fictional factory profile',users[1].id).run();
    expect((await fresh(1)).result?.user.name).toBe('Updated fictional factory profile');
    const pos=await createAuth('http://pos.localhost:3100');
    expect((await pos.api.getSession({headers:cookies[0]}))?.user.id??null).toBeNull();
    authRuntime.BETTER_AUTH_SECRET='different-private-fixture-secret-with-thirty-two-characters';
    expect((await fresh(0)).result?.user.id??null).toBeNull();
    authRuntime.BETTER_AUTH_SECRET=originalSecret;
    expect((await fresh(0)).result?.user.id).toBe(users[0].id);
    // Same origin/secret, genuinely different isolated database: no old session.
    await applyMigrations(env.CONTROL_READ_DB,env.CONTROL_TEST_MIGRATIONS);
    authRuntime.CONTROL_DB=env.CONTROL_READ_DB;
    const foreign=await fresh(0);expect(foreign.metrics.queries).toBe(1);expect(foreign.result?.user.id??null).toBeNull();
    authRuntime.CONTROL_DB=originalDatabase;
    expect((await fresh(0)).result?.user.id).toBe(users[0].id);
    await env.CONTROL_DB.prepare('UPDATE session SET "expiresAt"=? WHERE "userId"=?').bind(new Date(Date.now()-1000).toISOString(),users[0].id).run();
    expect((await fresh(0)).result?.user.id??null).toBeNull();
    expect((await fresh(1)).result?.user.id).toBe(users[1].id);
    await env.CONTROL_DB.prepare('DELETE FROM session WHERE "userId"=?').bind(users[1].id).run();
    expect((await fresh(1)).result?.user.id??null).toBeNull();
  }finally{authRuntime.CONTROL_DB=originalDatabase;authRuntime.BETTER_AUTH_SECRET=originalSecret;vi.unstubAllEnvs();}
});

it("observes changed signup policy and keeps legal invitation allowlists independent from ordinary auth initialization", async()=>{
  const originalFetch=globalThis.fetch,originalBeta=authRuntime.ERPX_PUBLIC_BETA;
  const deliveries:string[]=[];
  vi.stubGlobal('fetch',vi.fn(async(_url:string|URL|Request,options?:RequestInit)=>{deliveries.push(JSON.parse(String(options?.body)).to);return Response.json({id:'fictional-invitation-mail'});}));
  try {
    const emailA=`${crypto.randomUUID()}@example.test`,emailB=`${crypto.randomUUID()}@example.test`;
    const signup=(auth:Awaited<ReturnType<typeof createAuth>>,email:string)=>auth.handler(new Request('http://localhost:3100/api/auth/sign-up/email',{method:'POST',headers:{'Content-Type':'application/json',Origin:'http://localhost:3100'},body:JSON.stringify({email,name:'Fictional invited staff',password:'Fictional-invite-password-2026!'})}));
    authRuntime.ERPX_PUBLIC_BETA='false';
    expect((await signup(await createAuth(),emailA)).status).not.toBe(200);
    const inviteA=await createAuth(undefined,undefined,emailA),inviteB=await createAuth(undefined,undefined,emailB);
    expect((await signup(inviteA,emailB)).status).not.toBe(200);
    expect(await env.CONTROL_DB.prepare('SELECT id FROM "user" WHERE email=?').bind(emailB).first()).toBeNull();
    expect((await signup(inviteA,emailA)).status).toBe(200);
    expect((await signup(inviteB,emailB)).status).toBe(200);
    expect(deliveries).toHaveLength(2);
    expect((await signup(await createAuth(),`${crypto.randomUUID()}@example.test`)).status).not.toBe(200);
    authRuntime.ERPX_PUBLIC_BETA='true';
    expect((await createAuth()).options.emailAndPassword?.disableSignUp).toBe(false);
  }finally{authRuntime.ERPX_PUBLIC_BETA=originalBeta;vi.stubGlobal('fetch',originalFetch);}
});

it('reevaluates explicitly selected OIDC configuration after the ordinary factory is warm',async()=>{
  const providers=await import("@erpx/server-identity_access/public/iam-auth");
  const config=vi.spyOn(providers,'directoryOidcConfig'),originalFetch=globalThis.fetch,id=crypto.randomUUID();
  const network=vi.fn(async()=>{throw new Error('UNEXPECTED_FICTIONAL_OIDC_NETWORK_REQUEST');});
  vi.stubGlobal('fetch',network);
  try {
    await createAuth();
    config.mockResolvedValue({providerId:`directory-${id}`,clientId:'fictional-client',clientSecret:'fictional-provider-secret',authorizationUrl:'https://issuer.example.test/authorize',tokenUrl:'https://issuer.example.test/token'});
    const first=await createAuth(undefined,id);await first.$context;
    config.mockRejectedValue(new Error('FICTIONAL_OIDC_WITHDRAWN'));
    let withdrawn=false;
    try{await createAuth(undefined,id);}catch(error){withdrawn=error instanceof Error&&error.message==='FICTIONAL_OIDC_WITHDRAWN';}
    expect(withdrawn).toBe(true);
    expect(config.mock.calls.map(args=>args[0])).toEqual([id,id]);
    expect(network).not.toHaveBeenCalled();
  }finally{config.mockRestore();vi.stubGlobal('fetch',originalFetch);}
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
    const expected = "https://erpx.example.test/holdings/acme/administration/usage";
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
