import { createHash, createHmac } from "node:crypto";
import { beforeAll, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  deliverAccountInvitation,
  promoteAccountInvitationDeliveries,
} from "../../../src/server/modules/identity_access/public/account-invitation-delivery";
import { sendBetaEmail } from "../../../src/server/modules/manage_erpx/public/beta-email";
import { accountInvitationList } from "../../../src/server/modules/identity_access/public/account-invitation-list";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";

const control = env.CONTROL_DB;
const holding = env.H001_DB;
const now = Date.parse("2026-09-21T12:00:00+07:00");

beforeAll(async () => {
  await applyMigrations(control, env.CONTROL_TEST_MIGRATIONS);
  await applyMigrations(holding, env.STAFF_TEST_MIGRATIONS);
});

async function invitation(state = "awaiting_approval", expiresAt = now + 86400000) {
  const invitationId = `invite-${crypto.randomUUID()}`;
  const jobId = `delivery-${crypto.randomUUID()}`;
  const token = createHmac("sha256", env.BETTER_AUTH_SECRET)
    .update(`invitation:${invitationId}`)
    .digest("hex");
  const tokenHash = createHash("sha256").update(token).digest("hex");
  await control
    .prepare(
      "INSERT INTO account_invitations(id,token_hash,holding_id,company_id,person_id,email,name,role,scope_kind,created_by,assignment_id,expires_at,created_at) VALUES(?,?,?,?,?,?,?,'{}','holding','owner','assignment',?,?)",
    )
    .bind(
      invitationId,
      tokenHash,
      "h001",
      "company-invitation-test",
      `person-${crypto.randomUUID()}`,
      "new.employee@example.test",
      "New Employee",
      expiresAt,
      now,
    )
    .run();
  await control
    .prepare(
      "INSERT INTO account_invitation_delivery_jobs(id,invitation_id,holding_id,company_id,state,next_attempt_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)",
    )
    .bind(
      jobId,
      invitationId,
      "h001",
      "company-invitation-test",
      state,
      now,
      now,
      now,
    )
    .run();
  return { invitationId, jobId, tokenHash };
}

async function review(invitationId: string, status: "approved" | "rejected") {
  await holding
    .prepare(
      "INSERT INTO iam_reviews(id,company_id,kind,payload,created_by,created_person,created_at,status,reason) VALUES(?,?,'invitation.authorize',?::jsonb,'reviewer','Reviewer',?,?,?)",
    )
    .bind(
      `review-${crypto.randomUUID()}`,
      "company-invitation-test",
      JSON.stringify({ invitationId }),
      now,
      status,
      "Independent review",
    )
    .run();
}

it("keeps the activation panel scoped to its owner, Holding, Company and Branch without mutating expired invitations", async () => {
  const f = await seedExpenseCompany(holding, env.H001_FILES);
  const valid = await invitation("sent", Date.now() + 86400000);
  const expired = await invitation("failed", Date.now() - 1000);
  const accepted = await invitation("sent", Date.now() - 1000);
  const legacy = await invitation("queued", Date.now() + 86400000);
  const otherBranch = await invitation("queued", Date.now() + 86400000);
  const otherCompany = await invitation("queued", Date.now() + 86400000);
  const otherHolding = await invitation("queued", Date.now() + 86400000);
  const items = [valid, expired, accepted, legacy, otherBranch, otherCompany, otherHolding];
  try {
    for (const item of [valid, expired, accepted, legacy, otherBranch, otherHolding])
      await control.prepare("UPDATE account_invitations SET company_id=?,branch_id=? WHERE id=?").bind(f.company, item === otherBranch ? "another-branch" : f.branch, item.invitationId).run();
    await control.prepare("UPDATE account_invitations SET status='accepted' WHERE id=?").bind(accepted.invitationId).run();
    await control.prepare("UPDATE account_invitations SET holding_id='h002' WHERE id=?").bind(otherHolding.invitationId).run();
    await control.prepare("DELETE FROM account_invitation_delivery_jobs WHERE id=?").bind(legacy.jobId).run();
    expect(await accountInvitationList(f.claimant, control)).toBeNull();
    const rows = await accountInvitationList({ ...f.owner, branchId: f.branch }, control);
    expect(rows?.map(row => row.id).sort()).toEqual([valid, expired, accepted, legacy].map(item => item.invitationId).sort());
    expect(rows?.find(row => row.id === expired.invitationId)).toMatchObject({ status: "expired", delivery_state: "failed" });
    expect(rows?.find(row => row.id === accepted.invitationId)).toMatchObject({ status: "accepted" });
    expect(rows?.find(row => row.id === legacy.invitationId)).toMatchObject({ delivery_state: null, attempts: null, error: null, sent_at: null });
    expect((await accountInvitationList({ ...f.owner, branchId: null }, control))?.map(row => row.id).sort()).toEqual([valid, expired, accepted, legacy, otherBranch].map(item => item.invitationId).sort());
    expect(await control.prepare("SELECT status FROM account_invitations WHERE id=?").bind(expired.invitationId).first("status")).toBe("pending");
    expect(rows?.some(row => row.id === otherCompany.invitationId || row.id === otherHolding.invitationId)).toBe(false);
  } finally {
    // This read-only panel proof must not leave jobs for a later delivery case.
    for (const item of items) {
      await control.prepare("DELETE FROM account_invitation_delivery_jobs WHERE invitation_id=?").bind(item.invitationId).run();
      await control.prepare("DELETE FROM account_invitations WHERE id=?").bind(item.invitationId).run();
    }
  }
});

it("waits for independent approval and sends the reproducible activation token once", async () => {
  const item = await invitation();
  await promoteAccountInvitationDeliveries(control, holding, "h001", now);
  expect(
    await control
      .prepare("SELECT state FROM account_invitation_delivery_jobs WHERE id=?")
      .bind(item.jobId)
      .first<string>("state"),
  ).toBe("awaiting_approval");

  await review(item.invitationId, "approved");
  await promoteAccountInvitationDeliveries(control, holding, "h001", now);
  const sent: { to: string; subject: string; url: string; key?: string }[] = [];
  expect(
    await deliverAccountInvitation(
      control,
      {
        ...env,
        ERPX_PUBLIC_ORIGIN: "https://erpx.example.test",
      },
      async (_environment, to, subject, url, key) => {
        sent.push({ to, subject, url, key });
        return "email-provider-test-1";
      },
      now,
    ),
  ).toBe(true);
  expect(sent).toHaveLength(1);
  expect(sent[0]).toMatchObject({
    to: "new.employee@example.test",
    subject: "Activate your ERPX account",
    key: item.jobId,
  });
  const deliveredToken = new URL(sent[0].url).searchParams.get("token")!;
  expect(createHash("sha256").update(deliveredToken).digest("hex")).toBe(
    item.tokenHash,
  );
  expect(
    await control
      .prepare("SELECT state FROM account_invitation_delivery_jobs WHERE id=?")
      .bind(item.jobId)
      .first<string>("state"),
  ).toBe("sent");
  expect(
    await control
      .prepare("SELECT provider_id FROM account_invitation_delivery_jobs WHERE id=?")
      .bind(item.jobId)
      .first<string>("provider_id"),
  ).toBe("email-provider-test-1");
  expect(
    await deliverAccountInvitation(control, env, async () => {}, now),
  ).toBe(false);
});

it("delivers through the explicit sandbox adapter with a durable provider receipt", async () => {
  const item = await invitation("queued");
  const sandboxEnv = {
    ...env,
    CONTROL_DB: control,
    ERPX_EMAIL_ADAPTER: "sandbox",
    ERPX_PUBLIC_ORIGIN: "https://erpx.example.test",
  };
  expect(await deliverAccountInvitation(control, sandboxEnv, (deliveryEnv, to, subject, url, key) =>
    sendBetaEmail(deliveryEnv as Parameters<typeof sendBetaEmail>[0], to, subject, url, key), now)).toBe(true);
  const job = await control
    .prepare("SELECT state,provider_id FROM account_invitation_delivery_jobs WHERE id=?")
    .bind(item.jobId)
    .first<{ state: string; provider_id: string }>();
  expect(job).toMatchObject({ state: "sent", provider_id: `sandbox-${item.jobId}` });
  const mail = await control
    .prepare("SELECT recipient,subject,body_text,provider_id FROM sandbox_email_deliveries WHERE delivery_key=?")
    .bind(item.jobId)
    .first<{ recipient: string; subject: string; body_text: string; provider_id: string }>();
  expect(mail).toMatchObject({
    recipient: "new.employee@example.test",
    subject: "Activate your ERPX account",
    provider_id: `sandbox-${item.jobId}`,
  });
  expect(mail?.body_text).toContain("https://erpx.example.test/activate?token=");
  expect(
    await sendBetaEmail(
      sandboxEnv,
      "new.employee@example.test",
      "Activate your ERPX account",
      mail!.body_text.split("\n\n")[1]!,
      item.jobId,
    ),
  ).toBe(`sandbox-${item.jobId}`);
  await expect(
    sendBetaEmail(
      sandboxEnv,
      "new.employee@example.test",
      "Activate your ERPX account",
      "https://erpx.example.test/activate?token=changed",
      item.jobId,
    ),
  ).rejects.toThrow("EMAIL_IDEMPOTENCY_CONFLICT");
  await expect(
    sendBetaEmail(
      { ...sandboxEnv, ERPX_ENVIRONMENT: "production" },
      "new.employee@example.test",
      "Activate your ERPX account",
      "https://erpx.example.test/activate?token=production-guard-test",
      "sandbox-production-guard",
    ),
  ).rejects.toThrow("EMAIL_SANDBOX_FORBIDDEN_IN_PRODUCTION");
});

it("cancels rejected and expired invitations before an email can leave", async () => {
  const rejected = await invitation();
  await review(rejected.invitationId, "rejected");
  await promoteAccountInvitationDeliveries(control, holding, "h001", now);
  expect(
    await control
      .prepare("SELECT state FROM account_invitation_delivery_jobs WHERE id=?")
      .bind(rejected.jobId)
      .first<string>("state"),
  ).toBe("cancelled");

  const expired = await invitation("queued", now - 1);
  let called = false;
  await deliverAccountInvitation(
    control,
    env,
    async () => {
      called = true;
    },
    now,
  );
  expect(called).toBe(false);
  expect(
    await control
      .prepare("SELECT state FROM account_invitation_delivery_jobs WHERE id=?")
      .bind(expired.jobId)
      .first<string>("state"),
  ).toBe("cancelled");
});

it("backs off transient errors and exposes a recoverable failed state", async () => {
  const item = await invitation("queued");
  await control
    .prepare("UPDATE account_invitation_delivery_jobs SET attempts=5 WHERE id=?")
    .bind(item.jobId)
    .run();
  await deliverAccountInvitation(
    control,
    { ...env, ERPX_PUBLIC_ORIGIN: "https://erpx.example.test" },
    async () => {
      throw new Error("provider unavailable");
    },
    now,
  );
  const failed = await control
    .prepare(
      "SELECT state,attempts,error,next_attempt_at FROM account_invitation_delivery_jobs WHERE id=?",
    )
    .bind(item.jobId)
    .first<{
      state: string;
      attempts: number;
      error: string;
      next_attempt_at: number;
    }>();
  expect(failed).toMatchObject({
    state: "failed",
    attempts: 6,
    error: "EMAIL_DELIVERY_FAILED",
  });
  expect(failed!.next_attempt_at).toBeGreaterThan(now);
});


it("delivers the same approved invitation token on the Account owner without extending its access or expiry", async () => {
  const item = await invitation("queued", Date.now() + 86400000);
  const keys = ["ERPX_ENVIRONMENT", "ERPX_PRODUCT_ENVIRONMENT", "ERPX_PRODUCT_ROUTING", "ERPX_PRODUCT_SSO", "ERPX_PUBLIC_ORIGIN"];
  const previous = keys.map(key => process.env[key]);
  try {
    for (const [key, value] of Object.entries({ ERPX_ENVIRONMENT: "local", ERPX_PRODUCT_ENVIRONMENT: "development", ERPX_PRODUCT_ROUTING: "enabled", ERPX_PRODUCT_SSO: "enabled", ERPX_PUBLIC_ORIGIN: "https://console-dev-erpx.jesadakorn.com" })) vi.stubEnv(key, value);
    const before = await control.prepare("SELECT token_hash,role,scope_kind,expires_at,status FROM account_invitations WHERE id=?").bind(item.invitationId).first();
    const sent: string[] = [];
    expect(await deliverAccountInvitation(control, { ...env, ERPX_PUBLIC_ORIGIN: "https://console-dev-erpx.jesadakorn.com" }, async (_env, _to, _subject, link) => { sent.push(link); return "account-invitation-fixture"; }, now)).toBe(true);
    expect(sent).toHaveLength(1);
    const link = new URL(sent[0]);
    expect(link.origin).toBe("https://account-dev-erpx.jesadakorn.com");expect(link.pathname).toBe("/activate");
    expect(createHash("sha256").update(link.searchParams.get("token")!).digest("hex")).toBe(item.tokenHash);
    expect(await control.prepare("SELECT token_hash,role,scope_kind,expires_at,status FROM account_invitations WHERE id=?").bind(item.invitationId).first()).toEqual(before);
  } finally {
    keys.forEach((key, index) => previous[index] === undefined ? delete process.env[key] : process.env[key] = previous[index]);
    await control.prepare("DELETE FROM account_invitation_delivery_jobs WHERE invitation_id=?").bind(item.invitationId).run();
    await control.prepare("DELETE FROM account_invitations WHERE id=?").bind(item.invitationId).run();
  }
});
