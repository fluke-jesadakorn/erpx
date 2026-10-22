import { hashPassword } from "better-auth/crypto";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { getRuntime, closeRuntime } from "@erpx/server-runtime/adapters/runtime/env";
import { seedExpenseCompany } from "../../fixtures/expenses/company";

// Deliberately restricted to this task's isolated local verification databases.
if (
  resolve(process.env.ERPX_DATA_DIR ?? "") !==
  resolve(".runtime/work-cards-e2e")
)
  throw Error("WORK_CARDS_TEST_DIRECTORY_REQUIRED");
const manifest = resolve(".runtime/work-cards-e2e/config/databases.json");
if (process.env.ERPX_DATABASE_MANIFEST && resolve(process.env.ERPX_DATABASE_MANIFEST) !== manifest)
  throw Error("WORK_CARDS_TEST_MANIFEST_REQUIRED");
process.env.ERPX_DATABASE_MANIFEST = manifest;
const env = getRuntime();
try {
  const now = Date.now(),
    password = "WorkCards-ERPX-2026!",
    hash = await hashPassword(password);
  const accounts: Record<
    string,
    {
      email: string;
      userId: string;
      assignmentId: string;
      company: string;
      branch: string;
      department: string;
    }
  > = {};
  for (const beta of [false, true]) {
    const f = await seedExpenseCompany(env.H001_DB, env.H001_FILES);
    const missing = await f.member(
      "missing",
      "employee",
      1,
      f.department,
      false,
    );
    await f.grant(f.it, "warehouse_manager");
    for (const [name, actor] of Object.entries({
      owner: f.owner,
      member: f.claimant,
      missing,
      handler: f.it,
    })) {
      const email = `${actor.userId}@example.test`;
      await env.CONTROL_DB.batch([
        env.CONTROL_DB.prepare(
          'INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?)',
        ).bind(actor.userId, name, email, now, now),
        env.CONTROL_DB.prepare(
          'INSERT INTO account(id,"accountId","providerId",issuer,"userId",password,"createdAt","updatedAt") VALUES(?,?,\'credential\',\'local:credential\',?,?,?,?)',
        ).bind(
          `account-${actor.userId}`,
          actor.userId,
          actor.userId,
          hash,
          now,
          now,
        ),
        env.CONTROL_DB.prepare(
          "INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)",
        ).bind(`principal-${actor.userId}`, actor.userId, name, now),
      ]);
      accounts[`${beta ? "beta-" : ""}${name}`] = {
        email,
        userId: actor.userId,
        assignmentId: actor.assignmentId,
        company: f.company,
        branch: f.branch,
        department: f.department,
      };
      if (beta)
        await env.H001_DB.prepare(
          "INSERT INTO beta_members(user_id,joined_at) VALUES(?,?)",
        )
          .bind(actor.userId, now)
          .run();
    }
    if (beta)
      await env.H001_DB.prepare(
        "INSERT INTO beta_policy(company_id,member_limit,branch_limit,ai_monthly_limit,storage_bytes,created_at) VALUES(?,100,10,100,1073741824,?)",
      )
        .bind(f.company, now)
        .run();
  }
  await mkdir(".scratch/work-cards", { recursive: true });
  await writeFile(
    ".scratch/work-cards/fixture.json",
    JSON.stringify({ password, accounts }, null, 2),
  );
  console.log("Created isolated Work Cards ERP/beta fixtures.");
} finally {
  await closeRuntime();
}
