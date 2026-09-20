import { beforeAll, it, expect, vi } from "vitest";
import { env, applyMigrations } from "./runtime-env";
const session = vi.hoisted(() => ({
  role: "holding_owner",
  holdingId: "billing_api_holding",
  platformRole: "platform_admin",
}));
vi.mock("../src/server/authorization", () => ({
  requireWorkContext: async (slug: string) => {
    if (slug !== "billing-api") throw new Error("FORBIDDEN");
    return {
      role: session.role,
      holdingId: session.holdingId,
      userId: "test-owner",
      email: "owner@example.test",
    };
  },
  requirePlatformContext: async () => ({
    role: session.platformRole,
    userId: "test-admin",
    email: "admin@example.test",
  }),
}));
vi.mock("../src/server/staff-policy", () => ({
  requireCompanyAuthority: async () => {},
}));
vi.mock("../src/server/runtime", () => ({
  runtimeEnv: async () => ({
    ...env,
    ERPX_DATA_DIR: ".runtime/tests/billing-api-files",
    ERPX_PUBLIC_ORIGIN: "http://localhost:3100",
  }),
  holdingResources: async () => ({ write: env.H001_DB }),
}));
import { billingHandler } from "../src/server/billing-api";
const customer = billingHandler(false),
  platform = billingHandler(true);
const ctx = (path: string[] = []) => ({
  params: Promise.resolve({ holdingSlug: "billing-api", path }),
});
beforeAll(async () => {
  await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS);
  await env.CONTROL_DB.pool.query(
    "INSERT INTO holding_bindings VALUES('billing_api_holding','billing-api','API test','W_BILL_API','R_BILL_API','F_BILL_API','active')",
  );
});
it("allows owner reads, denies ordinary users and cross-Holding URL access", async () => {
  expect(
    (
      await customer(
        new Request(
          "http://localhost:3100/api/v1/holdings/billing-api/billing",
        ),
        ctx(),
      )
    ).status,
  ).toBe(200);
  session.role = "employee";
  expect(
    (
      await customer(
        new Request(
          "http://localhost:3100/api/v1/holdings/billing-api/billing",
        ),
        ctx(),
      )
    ).status,
  ).toBe(403);
  session.role = "holding_owner";
  expect(
    (
      await customer(
        new Request("http://localhost:3100/api/v1/holdings/other/billing"),
        { params: Promise.resolve({ holdingSlug: "other" }) },
      )
    ).status,
  ).toBe(403);
  expect(
    (
      await customer(
        new Request(
          "http://localhost:3100/api/v1/holdings/billing-api/billing/evidence/guessed",
        ),
        ctx(["evidence", "guessed"]),
      )
    ).status,
  ).toBe(403);
});
it("protects Platform mutations from auditors and cross-origin requests", async () => {
  session.platformRole = "platform_auditor";
  const request = () =>
    new Request(
      "http://localhost:3100/api/v1/platform/billing?holdingId=billing_api_holding",
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          action: "complete-costs",
          version: 0,
          reason: "Test review",
        }),
      },
    );
  expect((await platform(request(), ctx())).status).toBe(403);
  session.platformRole = "platform_admin";
  const evil = new Request(request(), {
    headers: {
      origin: "https://evil.example",
      "Content-Type": "application/json",
    },
  });
  expect((await platform(evil, ctx())).status).toBe(403);
  expect((await platform(request(), ctx())).status).toBe(422);
  expect(
    (
      await platform(
        new Request(
          "http://localhost:3100/api/v1/platform/billing/evidence/other?holdingId=billing_api_holding",
        ),
        ctx(["evidence", "other"]),
      )
    ).status,
  ).toBe(404);
});
it("stores evidence privately, retries uploads safely and prevents evidence access under another Holding", async () => {
  await env.CONTROL_DB.pool.query(
    "INSERT INTO billing_accounts(holding_id,created_at) VALUES($1,1) ON CONFLICT DO NOTHING",
    [session.holdingId],
  );
  const key = crypto.randomUUID(),
    bytes = Buffer.from(
      "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j7XcAAAAASUVORK5CYII=",
      "base64",
    );
  const upload = () =>
    platform(
      new Request(
        "http://localhost:3100/api/v1/platform/billing/evidence?holdingId=billing_api_holding",
        {
          method: "POST",
          headers: {
            "Content-Type": "image/png",
            "X-Filename": "fixture.png",
            "Idempotency-Key": key,
          },
          body: bytes,
        },
      ),
      ctx(["evidence"]),
    );
  const first = await upload();
  expect(first.status).toBe(200);
  const id = (await first.json()).data.id;
  expect((await (await upload()).json()).data.id).toBe(id);
  const file = await platform(
    new Request(
      `http://localhost:3100/api/v1/platform/billing/evidence/${id}?holdingId=billing_api_holding`,
    ),
    ctx(["evidence", id]),
  );
  expect(file.status).toBe(200);
  expect(file.headers.get("Cache-Control")).toBe("private, no-store");
  expect(file.headers.get("Content-Disposition")).toContain("attachment");
  expect(Buffer.from(await file.arrayBuffer())).toEqual(bytes);
  await env.CONTROL_DB.pool.query(
    "INSERT INTO holding_bindings VALUES('billing_other','billing-other','Other','W_B_OTHER','R_B_OTHER','F_B_OTHER','active')",
  );
  expect(
    (
      await platform(
        new Request(
          `http://localhost:3100/api/v1/platform/billing/evidence/${id}?holdingId=billing_other`,
        ),
        ctx(["evidence", id]),
      )
    ).status,
  ).toBe(404);
  expect(
    (
      await customer(
        new Request(
          `http://localhost:3100/api/v1/holdings/billing-api/billing/evidence/${id}`,
        ),
        ctx(["evidence", id]),
      )
    ).status,
  ).toBe(403);
});
