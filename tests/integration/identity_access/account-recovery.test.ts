import { createHash } from "node:crypto";
import { beforeAll, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  deliverAccountRecovery,
  queueAccountRecovery,
  recordAccountRecoveryCompleted,
} from "../../../src/server/modules/identity_access/public/account-recovery";
import { sendBetaEmail } from "../../../src/server/modules/manage_erpx/public/beta-email";

const db = env.CONTROL_DB;
const secret = "account-recovery-test-secret-at-least-32-chars";
const configuration = { ...env, BETTER_AUTH_SECRET: secret };
const now = Date.parse("2026-09-21T15:30:00Z");

beforeAll(() => applyMigrations(db, env.CONTROL_TEST_MIGRATIONS));

async function user() {
  const id = `recovery-user-${crypto.randomUUID()}`;
  const email = `${id}@example.test`;
  await db
    .prepare(
      'INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?)',
    )
    .bind(id, "Recovery User", email, now, now)
    .run();
  return { id, name: "Recovery User", email, emailVerified: true, createdAt: new Date(now), updatedAt: new Date(now) };
}

async function verification(userId: string, token: string) {
  await db
    .prepare(
      'INSERT INTO verification(id,identifier,value,"expiresAt","createdAt","updatedAt") VALUES(?,?,?,?,?,?)',
    )
    .bind(
      crypto.randomUUID(),
      `reset-password:${token}`,
      userId,
      now + 3600000,
      now,
      now,
    )
    .run();
}

it("replaces earlier recovery links and delivers only the current encrypted URL", async () => {
  const account = await user();
  const first = "first-token-1234567890";
  const second = "second-token-1234567890";
  await verification(account.id, first);
  await queueAccountRecovery(
    db,
    configuration,
    account,
    `https://erpx.example.test/api/auth/reset-password/${first}?callbackURL=%2Freset-password`,
    first,
    now,
  );
  await verification(account.id, second);
  await queueAccountRecovery(
    db,
    configuration,
    account,
    `https://erpx.example.test/api/auth/reset-password/${second}?callbackURL=%2Freset-password`,
    second,
    now + 1,
  );
  expect(
    await db
      .prepare("SELECT count(*)::int count FROM verification WHERE value=?")
      .bind(account.id)
      .first<number>("count"),
  ).toBe(1);
  expect(
    await db
      .prepare(
        "SELECT count(*)::int count FROM account_recovery_delivery_jobs WHERE user_id=? AND state='cancelled'",
      )
      .bind(account.id)
      .first<number>("count"),
  ).toBe(1);

  const sent: { to: string; url: string; key?: string }[] = [];
  await deliverAccountRecovery(
    db,
    configuration,
    async (_environment, to, _subject, url, key) => {
      sent.push({ to, url, key });
      return "provider-recovery-1";
    },
    now + 2,
  );
  expect(sent).toHaveLength(1);
  expect(sent[0].to).toBe(account.email);
  expect(sent[0].url).toContain(second);
  expect(sent[0].url).not.toContain(first);
  expect(
    await db
      .prepare(
        "SELECT provider_id FROM account_recovery_delivery_jobs WHERE user_id=? AND state='sent'",
      )
      .bind(account.id)
      .first<string>("provider_id"),
  ).toBe("provider-recovery-1");
  const ciphertext = await db
    .prepare(
      "SELECT url_ciphertext FROM account_recovery_delivery_jobs WHERE user_id=? AND state='sent'",
    )
    .bind(account.id)
    .first<string>("url_ciphertext");
  expect(ciphertext).not.toContain(second);
  expect(createHash("sha256").update(ciphertext!).digest("hex")).toHaveLength(64);
});

it("uses the explicit sandbox adapter for recovery and keeps the reset URL in the durable receipt", async () => {
  const account = await user();
  const token = "sandbox-recovery-token-1234567890";
  await verification(account.id, token);
  await queueAccountRecovery(
    db,
    { ...configuration, CONTROL_DB: db, ERPX_EMAIL_ADAPTER: "sandbox" },
    account,
    `https://erpx.example.test/reset/${token}`,
    token,
    now,
  );
  expect(
    await deliverAccountRecovery(
      db,
      { ...configuration, CONTROL_DB: db, ERPX_EMAIL_ADAPTER: "sandbox" },
      sendBetaEmail,
      now + 1,
    ),
  ).toBe(true);
  const mail = await db
    .prepare("SELECT recipient,subject,body_text,provider_id FROM sandbox_email_deliveries WHERE recipient=? AND subject=? ORDER BY created_at DESC LIMIT 1")
    .bind(account.email, "Reset your ERPX password")
    .first<{ recipient: string; subject: string; body_text: string; provider_id: string }>();
  expect(mail).toMatchObject({
    recipient: account.email,
    subject: "Reset your ERPX password",
    provider_id: expect.stringContaining("sandbox-account-recovery-"),
  });
  expect(mail?.body_text).toContain(`/reset/${token}`);
});

it("records bounded provider failure and immutable completion evidence", async () => {
  const account = await user();
  const token = "failure-token-1234567890";
  await verification(account.id, token);
  await queueAccountRecovery(
    db,
    configuration,
    account,
    `https://erpx.example.test/reset/${token}`,
    token,
    now,
  );
  await db
    .prepare(
      "UPDATE account_recovery_delivery_jobs SET attempts=5 WHERE user_id=?",
    )
    .bind(account.id)
    .run();
  await deliverAccountRecovery(
    db,
    configuration,
    async () => {
      throw new Error("provider unavailable");
    },
    now + 1,
  );
  expect(
    await db
      .prepare(
        "SELECT state FROM account_recovery_delivery_jobs WHERE user_id=?",
      )
      .bind(account.id)
      .first<string>("state"),
  ).toBe("failed");
  await recordAccountRecoveryCompleted(db, account.id, now + 2);
  expect(
    await db
      .prepare(
        "SELECT count(*)::int count FROM account_recovery_events WHERE user_id=? AND action IN ('requested','delivery_failed','completed')",
      )
      .bind(account.id)
      .first<number>("count"),
  ).toBe(3);
  const eventId = await db
    .prepare(
      "SELECT id FROM account_recovery_events WHERE user_id=? LIMIT 1",
    )
    .bind(account.id)
    .first<string>("id");
  await expect(
    db.prepare("DELETE FROM account_recovery_events WHERE id=?").bind(eventId).run(),
  ).rejects.toThrow(/Immutable IAM evidence/);
});


it("moves a queued Console recovery delivery to Account without replacing its token, ciphertext, expiry or verification", async () => {
  const account = await user(), token = "migration-recovery-" + crypto.randomUUID();
  const keys = ["ERPX_ENVIRONMENT", "ERPX_PUBLIC_ORIGIN", "ERPX_PRODUCT_ENVIRONMENT", "ERPX_PRODUCT_ROUTING", "ERPX_PRODUCT_SSO"];
  const previous = keys.map(key => process.env[key]);
  try {
    vi.stubEnv("ERPX_ENVIRONMENT", "local");
    vi.stubEnv("ERPX_PUBLIC_ORIGIN", "https://console-dev-erpx.jesadakorn.com");
    vi.stubEnv("ERPX_PRODUCT_ENVIRONMENT", "development");
    vi.stubEnv("ERPX_PRODUCT_ROUTING", "disabled");
    vi.stubEnv("ERPX_PRODUCT_SSO", "enabled");
    await verification(account.id, token);
    const original = `https://console-dev-erpx.jesadakorn.com/api/auth/reset-password/${token}?callbackURL=` + encodeURIComponent("https://console-dev-erpx.jesadakorn.com/reset-password");
    await queueAccountRecovery(db, configuration, account, original, token, now);
    const before = await db.prepare("SELECT id,verification_identifier,url_ciphertext,expires_at FROM account_recovery_delivery_jobs WHERE user_id=?").bind(account.id).first();
    vi.stubEnv("ERPX_PRODUCT_ROUTING", "enabled");
    const sent: string[] = [];
    expect(await deliverAccountRecovery(db, configuration, async (_env, _to, _subject, link) => { sent.push(link); return "migrated-recovery-fixture"; }, now + 1)).toBe(true);
    expect(sent).toHaveLength(1);
    const link = new URL(sent[0]);
    expect(link.origin).toBe("https://account-dev-erpx.jesadakorn.com");
    expect(link.pathname).toBe(`/api/auth/reset-password/${token}`);
    expect(link.searchParams.get("callbackURL")).toBe("https://account-dev-erpx.jesadakorn.com/reset-password");
    expect(await db.prepare("SELECT id,verification_identifier,url_ciphertext,expires_at FROM account_recovery_delivery_jobs WHERE user_id=?").bind(account.id).first()).toEqual(before);
    expect(await db.prepare("SELECT value FROM verification WHERE identifier=?").bind(`reset-password:${token}`).first("value")).toBe(account.id);
    expect(await db.prepare('SELECT count(*)::int n FROM session WHERE "userId"=?').bind(account.id).first("n")).toBe(0);
  } finally {
    keys.forEach((key, index) => previous[index] === undefined ? delete process.env[key] : process.env[key] = previous[index]);
  }
});
