import { createHash, createHmac } from "node:crypto";
import { beforeAll, expect, it } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import {
  deliverAccountInvitation,
  promoteAccountInvitationDeliveries,
} from "../src/server/account-invitation-delivery";

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
