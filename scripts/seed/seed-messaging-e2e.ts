import { hashPassword } from "better-auth/crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { getRuntime, closeRuntime } from "@erpx/server-runtime/adapters/runtime/env";
import { seedExpenseCompany } from "../../fixtures/expenses/company";

// Dedicated synthetic accounts in a separate company; never reuse real recipients.
const manifest = JSON.parse(
  await readFile(process.env.ERPX_DATABASE_MANIFEST!, "utf8"),
);
for (const connection of Object.values(manifest.databases)) {
  const url = new URL(connection as string);
  if (
    url.hostname !== "127.0.0.1" ||
    url.port !== "5432" ||
    !url.pathname.startsWith("/erpx_dev_")
  )
    throw new Error("LOCAL_DEV_DATABASE_REQUIRED");
}
const env = getRuntime();
try {
  const f = await seedExpenseCompany(env.H001_DB, env.H001_FILES),
    now = Date.now(),
    password = `Messaging-${crypto.randomUUID()}!`,
    hash = await hashPassword(password);
  await env.H001_DB.prepare(
    "UPDATE organization_nodes SET display_name='Messaging acceptance tests' WHERE id=?",
  )
    .bind(f.company)
    .run();
  const actors = { alice: f.claimant, bob: f.head, carol: f.reviewer };
  for (const [name, actor] of Object.entries(actors)) {
    actor.name = `Messaging ${name[0].toUpperCase() + name.slice(1)}`;
    actor.email = `${actor.userId}@example.test`;
    await env.CONTROL_DB.batch([
      env.CONTROL_DB.prepare(
        'INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)',
      ).bind(actor.userId, actor.name, actor.email, now, now),
      env.CONTROL_DB.prepare(
        "INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)",
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
      ).bind(`principal-${actor.userId}`, actor.userId, actor.name, now),
    ]);
  }
  // Exercise the same beta capability gate as the public workspace.
  await env.H001_DB.prepare(
    "INSERT INTO beta_policy(company_id,member_limit,branch_limit,ai_monthly_limit,storage_bytes,created_at) VALUES(?,100,10,100,1073741824,?)",
  )
    .bind(f.company, now)
    .run();
  await mkdir(".scratch/messaging", { recursive: true });
  await writeFile(
    ".scratch/messaging/fixture.json",
    JSON.stringify({ company: f.company, password, actors }),
    { mode: 0o600 },
  );
  console.log(
    "Created isolated messaging test accounts in .scratch/messaging/fixture.json",
  );
} finally {
  await closeRuntime();
}
