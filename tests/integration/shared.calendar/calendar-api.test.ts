import { beforeAll, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {
  GET,
  POST,
} from "@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/calendar/route";
import { GET as hierarchy } from "@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/calendar/[...path]/route";
let actor: WorkContext;
const resourcesObserved=vi.hoisted(()=>vi.fn());
vi.mock("@erpx/server-identity_access/public/authorization", () => ({
  requireWorkContext: async () => actor,
}));
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => {resourcesObserved();return { write: env.H001_DB, files: env.H001_FILES };},
  runtimeEnv: async () => env,
}));
beforeAll(async () => {
  await applyMigrations(env.H001_DB, env.STAFF_TEST_MIGRATIONS);
});
const context = { params: Promise.resolve({ holdingSlug: "test" }) };
it("rejects a manual Calendar command on People before acquiring its Holding database",async()=>{
 const f=await seedExpenseCompany(env.H001_DB,env.H001_FILES);actor=f.claimant;
 vi.stubEnv("ERPX_PRODUCT_ROUTING","enabled");vi.stubEnv("ERPX_PRODUCT_SSO","enabled");vi.stubEnv("ERPX_PRODUCT_ENVIRONMENT","development");vi.stubEnv("ERPX_ENVIRONMENT","local");resourcesObserved.mockClear();
 try{
  const response=await POST(new Request("https://people-dev-erpx.jesadakorn.com/api/v1/holdings/test/calendar",{method:"POST",headers:{host:"people-dev-erpx.jesadakorn.com",origin:"https://people-dev-erpx.jesadakorn.com","Content-Type":"application/json","Idempotency-Key":crypto.randomUUID()},body:JSON.stringify({action:"announcement.save",title:"Foreign owner command"})}),context);
  expect(response.status).toBe(404);expect(await response.json()).toMatchObject({error:{code:"NOT_FOUND"}});expect(resourcesObserved).not.toHaveBeenCalled();
 }finally{vi.unstubAllEnvs();}
});
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
