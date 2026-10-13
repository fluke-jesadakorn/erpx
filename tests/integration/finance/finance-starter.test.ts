import { beforeAll, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import {
  createFinanceStarter,
  financeSetupAuthority,
} from "../../../src/server/modules/finance/public/finance-starter";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
async function fresh() {
  const f = await expenseFixture(),
    company = `starter-${key()}`,
    assignment = key(),
    user = `${company}-finance`;
  await db.batch([
    db
      .prepare(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Starter fixture',?)",
      )
      .bind(company, company, company),
    db
      .prepare(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'North branch',?,?,?)",
      )
      .bind(
        `${company}-branch`,
        `${company}-branch`,
        company,
        company,
        `${company}-branch`,
      ),
    db
      .prepare(
        "INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,'finance_preparer','[]',?,0,9999999999999,'holding')",
      )
      .bind(assignment, user, company),
  ]);
  const actor = {
    ...f.owner,
    companyId: company,
    companySlug: company,
    userId: user,
    personId: null,
    role: "finance_preparer",
    assignmentId: assignment,
    scope: { ...f.owner.scope, companyId: company },
  } as WorkContext;
  return { actor, company };
}
const request = {
  preset: "thai_services_v1" as const,
  expectedVersion: 0 as const,
  name: "Reviewed Company starting ledger",
  firstMonth: "2028-02",
  organizationDimensions: true,
  reason:
    "Review the shown starting accounts before independent Company policy approval",
};
it("creates configuration once, with exact control categories and leap-month dates, without posting money or approving policy", async () => {
  const f = await fresh(),
    command = key(),
    result = await createFinanceStarter(f.actor, request, command);
  expect(result).toMatchObject({
    currency: "THB",
    accountCount: 23,
    policyReviewRequired: true,
  });
  expect(await createFinanceStarter(f.actor, request, command)).toEqual(result);
  expect(
    await db
      .prepare("SELECT start_date,end_date FROM finance_periods WHERE id=?")
      .bind(result.periodId)
      .first(),
  ).toEqual({ start_date: "2028-02-01", end_date: "2028-02-29" });
  expect(
    (
      await db
        .prepare(
          "SELECT code,category,control_kind FROM finance_accounts WHERE chart_id=? AND control_kind<>'none' ORDER BY code",
        )
        .bind(result.id)
        .all()
    ).results,
  ).toEqual([
    { code: "1100", category: "asset", control_kind: "bank" },
    { code: "1200", category: "asset", control_kind: "receivable" },
    { code: "2100", category: "liability", control_kind: "payable" },
  ]);
  expect(
    await db
      .prepare(
        "SELECT required FROM finance_dimension_definitions WHERE company_id=? AND code='branch'",
      )
      .bind(f.company)
      .first("required"),
  ).toBe(0);
  expect(
    await db
      .prepare(
        "SELECT code,name FROM finance_dimension_values WHERE company_id=?",
      )
      .bind(f.company)
      .first(),
  ).toEqual({ code: `${f.company}-branch`, name: "North branch" });
  for (const table of [
    "finance_journals",
    "finance_financial_events",
    "finance_accounting_profiles",
    "finance_company_tools",
  ])
    expect(
      await db
        .prepare(`SELECT count(*) n FROM ${table} WHERE company_id=?`)
        .bind(f.company)
        .first("n"),
    ).toBe(0);
  await expect(
    createFinanceStarter(f.actor, request, key()),
  ).rejects.toMatchObject({ code: "STARTER_ALREADY_CONFIGURED" });
});
it("rolls back the entire configuration on conflicting dimensions and refuses cross-Company or branch authority", async () => {
  const f = await fresh();
  await db
    .prepare(
      "INSERT INTO finance_dimension_definitions(id,company_id,code,name,created_by,created_at) VALUES(?,?,'branch','Prior dimension','fixture',1)",
    )
    .bind(key(), f.company)
    .run();
  await expect(createFinanceStarter(f.actor, request, key())).rejects.toThrow();
  expect(
    await db
      .prepare("SELECT count(*) n FROM finance_charts WHERE company_id=?")
      .bind(f.company)
      .first("n"),
  ).toBe(0);
  expect(
    await db
      .prepare("SELECT count(*) n FROM finance_accounts WHERE company_id=?")
      .bind(f.company)
      .first("n"),
  ).toBe(0);
  expect(
    await db
      .prepare("SELECT count(*) n FROM finance_periods WHERE company_id=?")
      .bind(f.company)
      .first("n"),
  ).toBe(0);
  const other = await fresh();
  await expect(
    createFinanceStarter(
      { ...f.actor, companyId: other.company },
      request,
      key(),
    ),
  ).rejects.toThrow();
  expect(
    await db
      .prepare("SELECT count(*) n FROM finance_charts WHERE company_id=?")
      .bind(other.company)
      .first("n"),
  ).toBe(0);
  await expect(
    createFinanceStarter(
      {
        ...other.actor,
        scope: { ...other.actor.scope, kind: "branch" } as WorkContext["scope"],
      },
      request,
      key(),
    ),
  ).rejects.toMatchObject({ code: "COMPANY_FINANCE_REQUIRED" });
});
it("leaves one complete starter after two concurrent preparations", async () => {
  const f = await fresh(),
    results = await Promise.allSettled([
      createFinanceStarter(f.actor, request, key()),
      createFinanceStarter(f.actor, request, key()),
    ]);
  expect(results.filter((r) => r.status === "fulfilled")).toHaveLength(1);
  expect(
    await db
      .prepare("SELECT count(*) n FROM finance_charts WHERE company_id=?")
      .bind(f.company)
      .first("n"),
  ).toBe(1);
  expect(
    await db
      .prepare("SELECT count(*) n FROM finance_accounts WHERE company_id=?")
      .bind(f.company)
      .first("n"),
  ).toBe(23);
});
it("checks current independent authority rather than counting role labels or a second assignment for the same person", async () => {
  const f = await fresh();
  expect(await financeSetupAuthority(db, f.company)).toMatchObject({
    preparerCount: 1,
    approverCount: 0,
    independent: false,
  });
  const self = key();
  await db
    .prepare(
      "INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,'finance_approver','[]',?,0,9999999999999,'holding')",
    )
    .bind(self, f.actor.userId, f.company)
    .run();
  expect((await financeSetupAuthority(db, f.company)).independent).toBe(false);
  const separate = key();
  await db
    .prepare(
      "INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,'finance_approver','[]',?,0,9999999999999,'holding')",
    )
    .bind(separate, `${f.company}-independent`, f.company)
    .run();
  expect((await financeSetupAuthority(db, f.company)).independent).toBe(true);
  await db
    .prepare("UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?")
    .bind(Date.now(), separate)
    .run();
  expect((await financeSetupAuthority(db, f.company)).independent).toBe(false);
});
it("rolls back the entire starter chart accounts period and imported dimensions after final outbox failure, then retries once",async()=>{
 const f=await fresh(),command=key(),fault=`starter_fault_${crypto.randomUUID().replaceAll("-","")}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'STARTER_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='finance.starter.create') EXECUTE FUNCTION ${fault}();`);
 try{
  await expect(createFinanceStarter(f.actor,request,command)).rejects.toThrow();
  for(const table of ["finance_charts","finance_accounts","finance_periods","finance_dimension_definitions","finance_dimension_values"])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE company_id=?`).bind(f.company).first("n"))).toBe(0);
  for(const table of ["command_receipts","audit_events","outbox_events"])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first("n"))).toBe(0);
 }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
 const result=await createFinanceStarter(f.actor,request,command);expect(await createFinanceStarter(f.actor,request,command)).toEqual(result);
 expect(result).toMatchObject({accountCount:23,policyReviewRequired:true});
 for(const [table,count] of [["finance_charts",1],["finance_accounts",23],["finance_periods",1],["finance_dimension_definitions",1],["finance_dimension_values",1]] as const)expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE company_id=?`).bind(f.company).first("n"))).toBe(count);
 for(const table of ["command_receipts","audit_events","outbox_events"])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first("n"))).toBe(1);
});
