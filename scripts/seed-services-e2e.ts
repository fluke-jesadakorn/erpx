import { getLocalProxy } from "./local/fixtures";
import { hashPassword } from "better-auth/crypto";
import { mkdir } from "node:fs/promises";
import { seedExpenseCompany } from "../fixtures/expenses/company";
// This script only opens local persisted D1/R2. It never changes existing users.
const beta = process.argv.includes("--beta"),
  fixturePath = `.scratch/shared-workspace/${beta ? "beta-" : ""}fixture.json`;
const proxy = await getLocalProxy({
  configPath: "wrangler.jsonc",
  persist: { path: ".wrangler/state/v3" },
});
try {
  const f = await seedExpenseCompany(proxy.env.H001_DB, proxy.env.H001_FILES),
    now = Date.now(),
    password = "Services-ERPX-2026!",
    hash = await hashPassword(password);
  const missing = await f.member("missing", "employee", 1, f.department, false);
  await f.grant(f.it, "purchasing_operator");
  await f.grant(f.it, "warehouse_manager");
  const product = `${f.company}-laptop`,
    warehouse = `${f.company}-warehouse`,
    supplier = `${f.company}-supplier`;
  await proxy.env.H001_DB.batch([
    proxy.env.H001_DB.prepare(
      "INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES(?,?,'LAPTOP','Office laptop',10000)",
    ).bind(product, f.company),
    proxy.env.H001_DB.prepare(
      "INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES(?,?,?,'OFFICE','Office stores')",
    ).bind(warehouse, f.company, f.branch),
    proxy.env.H001_DB.prepare(
      "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'supplier','SUPPLIER','Office supplier','THB',?,?)",
    ).bind(supplier, f.company, now, now),
  ]);
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
  await mkdir(".scratch/shared-workspace", { recursive: true });
  if (beta) {
    await proxy.env.H001_DB.prepare(
      "INSERT INTO beta_policy(company_id,member_limit,branch_limit,ai_monthly_limit,storage_bytes,created_at) VALUES(?,100,10,100,1073741824,?)",
    ).bind(f.company, now).run();
    for (const actor of Object.values(actors))
      await proxy.env.H001_DB.prepare(
        "INSERT INTO beta_members(user_id,joined_at) VALUES(?,?)",
      ).bind(actor.userId, now).run();
  }
  await Bun.write(
    fixturePath,
    JSON.stringify(
      {
        company: f.company,
        beta,
        product,
        warehouse,
        supplier,
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
    `Created isolated local employee-service E2E fixtures in ${fixturePath}.`,
  );
} finally {
  await proxy.dispose();
}
