import { changeAppointment } from "../src/server/department-authority";
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
  // The browser acceptance company must have the same explicit organization
  // graph as the working expense fixtures: six numbered levels, positions,
  // and reporting lines. Without this setup the real route correctly blocks
  // an otherwise complete claim before the first approval step.
  await proxy.env.H001_DB.batch(
    Array.from({ length: 6 }, (_, index) => index + 1).map((number) =>
      proxy.env.H001_DB
        .prepare(
          "INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,active,created_at,updated_at) VALUES(?,?,?,?,?,?,1,?,?) ON CONFLICT DO NOTHING",
        )
        .bind(
          `org-level-${f.company}-${number}`,
          f.company,
          `ORG_L${number}`,
          [
            "CEO",
            "Executive / Director",
            "Branch / Department Manager",
            "Supervisor",
            "Senior Staff",
            "Staff",
          ][number - 1],
          -number,
          number,
          now,
          now,
        ),
    ),
  );
  const positions = [
    ["ceo", 1, null, 1],
    ["head", 3, "ceo", 0],
    ["manager", 4, "ceo", 0],
    ["staff", 5, "ceo", 0],
    ["claimant", 6, "head", 0],
  ] as const;
  await proxy.env.H001_DB.batch(
    positions.map(([name, level, reportsTo, isCeo]) =>
      proxy.env.H001_DB
        .prepare(
          "INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,reports_to_position_id,is_ceo,active,version,created_by,created_at,updated_at) VALUES(?,?,NULL,NULL,?,?,?,?,1,1,?,?,?) ON CONFLICT DO NOTHING",
        )
        .bind(
          `${f.company}-position-${name}`,
          f.company,
          name === "ceo" ? "CEO" : name === "claimant" ? "Staff member" : `${name} reviewer`,
          `org-level-${f.company}-${level}`,
          reportsTo ? `${f.company}-position-${reportsTo}` : null,
          isCeo,
          f.owner.userId,
          now,
          now,
        ),
    ),
  );
  await proxy.env.H001_DB.batch([
    proxy.env.H001_DB
      .prepare(
        "UPDATE hr_engagements SET level_id=?,position_id=?,manager_person_id=NULL WHERE id=? AND company_id=?",
      )
      .bind(
        `org-level-${f.company}-1`,
        `${f.company}-position-ceo`,
        `eng-${f.owner.userId}`,
        f.company,
      ),
    proxy.env.H001_DB
      .prepare(
        "UPDATE hr_engagements SET level_id=?,position_id=?,manager_person_id=? WHERE id IN (?,?) AND company_id=?",
      )
      .bind(
        `org-level-${f.company}-3`,
        `${f.company}-position-head`,
        `person-${f.owner.userId}`,
        `eng-${f.head.userId}`,
        `eng-${f.tie.userId}`,
        f.company,
      ),
    proxy.env.H001_DB
      .prepare(
        "UPDATE hr_engagements SET level_id=?,position_id=?,manager_person_id=? WHERE id=? AND company_id=?",
      )
      .bind(
        `org-level-${f.company}-4`,
        `${f.company}-position-manager`,
        `person-${f.owner.userId}`,
        `eng-${f.manager.userId}`,
        f.company,
      ),
    ...[f.reviewer, f.payer, f.it].map((actor) =>
      proxy.env.H001_DB
        .prepare(
          "UPDATE hr_engagements SET level_id=?,position_id=?,manager_person_id=? WHERE id=? AND company_id=?",
        )
        .bind(
          `org-level-${f.company}-5`,
          `${f.company}-position-staff`,
          `person-${f.owner.userId}`,
          `eng-${actor.userId}`,
          f.company,
        ),
    ),
    proxy.env.H001_DB
      .prepare(
        "UPDATE hr_engagements SET level_id=?,position_id=?,manager_person_id=? WHERE id=? AND company_id=?",
      )
      .bind(
        `org-level-${f.company}-6`,
        `${f.company}-position-claimant`,
        `person-${f.head.userId}`,
        `eng-${f.claimant.userId}`,
        f.company,
      ),
  ]);
  await changeAppointment(proxy.env.H001_DB,f.owner,{kind:"head",departmentId:f.department,positionId:null,personId:`person-${f.head.userId}`,expectedVersion:0,reason:"Appointed head for isolated development browser tests"},crypto.randomUUID());
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
