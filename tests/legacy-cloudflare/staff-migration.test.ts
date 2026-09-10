import { env } from "cloudflare:workers";
import { applyD1Migrations, type D1Migration } from "cloudflare:test";
import { it, expect } from "vitest";
it("adds blocked routes only to pending records without fabricating historical decisions", async () => {
  const db = env.H002_DB,
    migrations = (env as typeof env & { STAFF_TEST_MIGRATIONS: D1Migration[] })
      .STAFF_TEST_MIGRATIONS;
  const boundary = migrations.findIndex((m) =>
    m.name.includes("0027_staff_work"),
  );
  expect(boundary).toBeGreaterThan(0);
  await applyD1Migrations(db, migrations.slice(0, boundary));
  await db.batch([
    db.prepare(
      "INSERT INTO employees(id,company_id,branch_id,display_name,created_at) VALUES('legacy-person','legacy-co','legacy-branch','Legacy staff',1)",
    ),
    ...["pending_approval", "approved"].map((state) =>
      db
        .prepare(
          "INSERT INTO leave_requests(id,number,employee_id,company_id,branch_id,start_date,days,state,version,created_by,decided_by,decision_note,created_at,updated_at) VALUES(?,?,'legacy-person','legacy-co','legacy-branch','2026-01-01',2,?,7,'legacy-user','legacy-head','Existing decision',1,2)",
        )
        .bind(state, state, state),
    ),
    db.prepare(
      "INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('legacy-chart','legacy-co','Legacy','THB','system',1,1)",
    ),
    db.prepare(
      "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES('legacy-account','legacy-chart','legacy-co','5000','Expense','expense','THB',1,1)",
    ),
    ...["in_review", "reimbursed"].map((status) =>
      db
        .prepare(
          "INSERT INTO finance_expenses(id,company_id,expense_date,merchant,currency,amount_minor,category_account_id,status,version,created_by,approved_by,created_at,updated_at) VALUES(?,'legacy-co','2026-01-01','Supplier','THB',100,'legacy-account',?,9,'legacy-user','legacy-head',1,2)",
        )
        .bind(status, status),
    ),
  ]);
  await db.batch([
    db.prepare(
      "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES('legacy-decision','expense.approve','legacy-head','historical','reimbursed','{}',1)",
    ),
    db.prepare(
      "INSERT INTO expense_decisions(id,expense_id,command_id,action,actor_id,reviewed_version,reason,created_at) VALUES('legacy-decision','reimbursed','legacy-decision','approve','legacy-head',8,'Original evidence reviewed',1)",
    ),
  ]);
  await applyD1Migrations(db, migrations);
  expect(
    (
      await db
        .prepare(
          "SELECT record_type,record_id,state FROM approval_instances ORDER BY record_type",
        )
        .all()
    ).results,
  ).toEqual([
    { record_type: "expense", record_id: "in_review", state: "blocked" },
    { record_type: "leave", record_id: "pending_approval", state: "blocked" },
  ]);
  expect(
    await db
      .prepare(
        "SELECT state,version,decided_by,decision_note FROM leave_requests WHERE id='approved'",
      )
      .first(),
  ).toEqual({
    state: "approved",
    version: 7,
    decided_by: "legacy-head",
    decision_note: "Existing decision",
  });
  expect(
    await db
      .prepare(
        "SELECT status,version,approved_by FROM finance_expenses WHERE id='reimbursed'",
      )
      .first(),
  ).toEqual({ status: "reimbursed", version: 9, approved_by: "legacy-head" });
  expect(
    await db.prepare("SELECT COUNT(*) count FROM approval_decisions").first(),
  ).toEqual({ count: 0 });
  expect((await db.prepare("PRAGMA foreign_key_check").all()).results).toEqual(
    [],
  );
  expect(
    await db
      .prepare(
        "SELECT reason,reviewed_version FROM expense_decisions WHERE id='legacy-decision'",
      )
      .first(),
  ).toEqual({ reason: "Original evidence reviewed", reviewed_version: 8 });
});
