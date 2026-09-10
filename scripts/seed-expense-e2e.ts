import { getLocalProxy } from "./local/fixtures";
import { hashPassword } from "better-auth/crypto";
import { mkdir } from "node:fs/promises";
import { seedExpenseCompany } from "../fixtures/expenses/company";
// This script only opens local persisted D1/R2. It never changes existing users.
const proxy = await getLocalProxy({
  configPath: "wrangler.jsonc",
  persist: { path: ".wrangler/state/v3" },
});
try {
  const f = await seedExpenseCompany(proxy.env.H001_DB, proxy.env.H001_FILES),
    now = Date.now(),
    password = "Expense-ERPX-2026!",
    hash = await hashPassword(password);
  const missing = await f.member("missing", "employee", 1, f.department, false);
  const actors = {
    claimant: f.claimant,
    head: f.head,
    reviewer: f.reviewer,
    manager: f.manager,
    payer: f.payer,
    owner: f.owner,
    it: f.it,
    missing,
  };
  for (const actor of Object.values(actors)) {
    actor.email = `${actor.userId}@example.test`;
    await proxy.env.CONTROL_DB.batch([
      proxy.env.CONTROL_DB.prepare(
        'INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)',
      ).bind(actor.userId, actor.name, actor.email, now, now),
      proxy.env.CONTROL_DB.prepare(
        "INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)",
      ).bind(
        `account-${actor.userId}`,
        actor.userId,
        actor.userId,
        hash,
        now,
        now,
      ),
      proxy.env.CONTROL_DB.prepare(
        "INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)",
      ).bind(`principal-${actor.userId}`, actor.userId, actor.name, now),
    ]);
    await proxy.env.H001_DB.prepare(
      "UPDATE hr_people SET email=? WHERE user_id=? AND company_id=?",
    )
      .bind(actor.email, actor.userId, f.company)
      .run();
  }
  await mkdir(".scratch/expense-v2", { recursive: true });
  await Bun.write(
    ".scratch/expense-v2/fixture.json",
    JSON.stringify(
      {
        company: f.company,
        branch: f.branch,
        department: f.department,
        accounts: f.accounts,
        period: f.period,
        password,
        actors,
      },
      null,
      2,
    ),
  );
  console.log(
    "Created isolated local expense E2E fixtures in .scratch/expense-v2/fixture.json.",
  );
} finally {
  await proxy.dispose();
}
