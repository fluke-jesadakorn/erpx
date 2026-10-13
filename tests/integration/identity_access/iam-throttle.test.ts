import { createHash, randomUUID } from "node:crypto";
import { beforeAll, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { iamThrottle } from "../../../src/server/modules/identity_access/public/iam-throttle";

vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({ runtimeEnv: async () => env }));
beforeAll(() => applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS));

it("allows exactly the configured number of concurrent attempts across one PostgreSQL counter", async () => {
  const identity = `${randomUUID()}@example.test`, category = `login-${randomUUID()}`;
  const results = await Promise.allSettled(Array.from({ length: 10 }, (_, index) => iamThrottle(category, index % 2 ? identity.toUpperCase() : identity, 3)));
  expect(results.filter(result => result.status === "fulfilled")).toHaveLength(3);
  const rejected = results.filter(result => result.status === "rejected");
  expect(rejected).toHaveLength(7);
  expect(rejected.every(result => result.status === "rejected" && result.reason.code === "IAM_RATE_LIMIT" && result.reason.status === 429)).toBe(true);
  const key = createHash("sha256").update(category).update(":").update(identity).digest("hex");
  expect(await env.CONTROL_DB.prepare("SELECT attempts FROM iam_throttles WHERE key=?").bind(key).first<number>("attempts")).toBe(10);
});

it("resets only an expired window and isolates independent categories", async () => {
  const category = `login-${randomUUID()}`, identity = `${randomUUID()}@example.test`;
  await iamThrottle(category, identity, 1);
  await expect(iamThrottle(category, identity, 1)).rejects.toMatchObject({ code: "IAM_RATE_LIMIT" });
  const key = createHash("sha256").update(category).update(":").update(identity).digest("hex");
  await env.CONTROL_DB.prepare("UPDATE iam_throttles SET window_start=? WHERE key=?").bind(Date.now() - 300001, key).run();
  await expect(iamThrottle(category, identity, 1)).resolves.toBeUndefined();
  await expect(iamThrottle(`mfa-${category}`, identity, 1)).resolves.toBeUndefined();
  expect(await env.CONTROL_DB.prepare("SELECT attempts FROM iam_throttles WHERE key=?").bind(key).first<number>("attempts")).toBe(1);
});
