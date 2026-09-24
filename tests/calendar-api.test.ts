import { beforeAll, expect, it, vi } from "vitest";
import { env, applyMigrations } from "./runtime-env";
import { seedExpenseCompany } from "../fixtures/expenses/company";
import type { WorkContext } from "../src/server/authorization";
import {
  GET,
  POST,
} from "../src/app/api/v1/holdings/[holdingSlug]/calendar/route";
import { GET as hierarchy } from "../src/app/api/v1/holdings/[holdingSlug]/calendar/[...path]/route";
let actor: WorkContext;
vi.mock("../src/server/authorization", () => ({
  requireWorkContext: async () => actor,
}));
vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
beforeAll(async () => {
  await applyMigrations(env.H001_DB, env.STAFF_TEST_MIGRATIONS);
});
const context = { params: Promise.resolve({ holdingSlug: "test" }) };
it("checks command keys, origin and input at the API boundary", async () => {
  const f = await seedExpenseCompany(env.H001_DB, env.H001_FILES);
  actor = f.claimant;
  const call = (body: unknown, headers: Record<string, string> = {}) =>
    POST(
      new Request("http://localhost:3100/api/v1/holdings/test/calendar", {
        method: "POST",
        headers: { "Content-Type": "application/json", ...headers },
        body: JSON.stringify(body),
      }),
      context,
    );
  expect((await call({})).status).toBe(422);
  expect(
    (
      await call(
        {},
        {
          Origin: "https://untrusted.example",
          "Idempotency-Key": crypto.randomUUID(),
        },
      )
    ).status,
  ).toBe(403);
  expect(
    (
      await call(
        {
          action: "announcement.save",
          audienceId: f.company,
          title: "Not authorized",
          content: "Notice",
          startDate: "2027-01-01",
          endDate: "2027-01-01",
        },
        { "Idempotency-Key": crypto.randomUUID() },
      )
    ).status,
  ).toBe(403);
  expect(
    (
      await GET(
        new Request(
          "http://localhost:3100/api/v1/holdings/test/calendar?date=invalid",
        ),
        context,
      )
    ).status,
  ).toBe(422);
  const response = await GET(
    new Request(
      "http://localhost:3100/api/v1/holdings/test/calendar?date=2027-01-01",
    ),
    context,
  );
  expect(response.status).toBe(200);
  expect(response.headers.get("cache-control")).toBe("private, no-store");
  const responseBody = await response.json();
  expect(responseBody.data.query.mode).toBe("mine");
  expect(responseBody.data.query.view).toBe("week");
  expect(responseBody.data.canAnnounce).toBe(false);
  const people = await hierarchy(
    new Request(
      "http://localhost:3100/api/v1/holdings/test/calendar/hierarchy?limit=1",
    ),
    { params: Promise.resolve({ holdingSlug: "test", path: ["hierarchy"] }) },
  );
  expect(people.status).toBe(200);
  expect((await people.json()).data.people).toHaveLength(1);
});
