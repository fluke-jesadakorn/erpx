import { createHash } from "node:crypto";
import { beforeAll, expect, it } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import {
  deliverAccountRecovery,
  queueAccountRecovery,
  recordAccountRecoveryCompleted,
} from "../src/server/account-recovery";

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
