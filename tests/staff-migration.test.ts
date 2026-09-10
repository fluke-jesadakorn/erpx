import { it, expect } from "vitest";
import { env, applyMigrations } from "./runtime-env";
it("creates a complete, empty PostgreSQL baseline and preserves database invariants", async () => {
  await applyMigrations(env.H001_DB, env.STAFF_TEST_MIGRATIONS);
  await applyMigrations(env.H001_DB, env.STAFF_TEST_MIGRATIONS);
  expect(
    await env.H001_DB.prepare(
      "SELECT COUNT(*) count FROM information_schema.tables WHERE table_schema='public' AND table_type='BASE TABLE'",
    ).first(),
  ).toEqual({ count: 135 });
  expect(
    await env.H001_DB.prepare("SELECT count(*) n FROM inventory_ledger").first(
      "n",
    ),
  ).toBe(0);
  for (const table of [
    "organization_nodes",
    "assignments",
    "finance_expenses",
    "leave_requests",
    "hr_payroll_runs",
    "command_receipts",
    "audit_events",
    "messaging_teams",
    "messaging_team_members",
    "messaging_rooms",
    "messaging_room_members",
    "messaging_messages",
    "messaging_reads",
    "messaging_files",
  ])
    expect(
      await env.H001_DB.prepare(`SELECT COUNT(*) count FROM ${table}`).first(),
    ).toEqual({ count: 0 });
  await expect(
    env.H001_DB.prepare(
      "INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES('invalid','user','holding_owner','[]','company',0,100,'branch')",
    ).run(),
  ).rejects.toMatchObject({ code: "23514" });
});
