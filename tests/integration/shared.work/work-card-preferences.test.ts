import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import {
  expenseFixture,
  expenseDb as db,
  key,
} from "../../support/helpers/expense-fixture";
import {
  readWorkCardPreferences,
  saveWorkCardPreferences,
} from "@erpx/server-work_projection/public/work-card-preferences";
import {
  getWorkCapabilities,
  hasCoveredTeam,
} from "@erpx/server-work_projection/public/work-capabilities";
import {saveServiceTeam,serviceTeamSchema} from '@erpx/server-support/public/service-teams';
import type { WorkCardId } from "@erpx/feature-work_projection/lib/work-cards";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/adapters/runtime/types";

vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let f: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
  f = await expenseFixture();
});
const available = async (): Promise<WorkCardId[]> => [
  "expenses",
  "services",
  "counts",
];
const save = (
  ids: WorkCardId[] | null,
  version: number,
  actor = f.claimant,
  id = key(),
) =>
  saveWorkCardPreferences(
    db,
    actor,
    { pinnedIds: ids, expectedVersion: version },
    id,
    available,
  );

it("distinguishes defaults from no pins and reads the same arrangement from another device", async () => {
  expect(await readWorkCardPreferences(db, f.claimant)).toEqual({
    pinnedIds: null,
    version: 0,
  });
  expect(await save([], 0)).toEqual({ pinnedIds: [], version: 1 });
  expect(await readWorkCardPreferences(db, { ...f.claimant })).toEqual({
    pinnedIds: [],
    version: 1,
  });
  await save(["services", "expenses"], 1);
  expect(await readWorkCardPreferences(db, f.claimant)).toEqual({
    pinnedIds: ["services", "expenses"],
    version: 2,
  });
  expect(await save(null, 2)).toEqual({ pinnedIds: null, version: 3 });
});
it("replays the exact save once and rejects reuse with different input", async () => {
  const id = key(),
    first = await save(["expenses"], 0, f.claimant, id);
  expect(await save(["expenses"], 0, f.claimant, id)).toEqual(first);
  await expect(save(["services"], 0, f.claimant, id)).rejects.toMatchObject({
    status: 409,
    code: "IDEMPOTENCY_CONFLICT",
  });
  expect(
    (
      await db
        .prepare("SELECT count(*) count FROM audit_events WHERE command_id=?")
        .bind(id)
        .first<{ count: number }>()
    )?.count,
  ).toBe(1);
});
it("allows only one concurrent arrangement for the same expected version", async () => {
  const results = await Promise.allSettled([
    save(["expenses"], 0),
    save(["services"], 0),
  ]);
  expect(results.filter((r) => r.status === "fulfilled")).toHaveLength(1);
  expect((await readWorkCardPreferences(db, f.claimant)).version).toBe(1);
  await expect(save([], 0)).rejects.toMatchObject({
    status: 409,
    code: "VERSION_CONFLICT",
  });
});
it("keeps preferences private to each company and user", async () => {
  await save(["expenses"], 0);
  expect((await readWorkCardPreferences(db, f.owner)).pinnedIds).toBeNull();
  const other = await expenseFixture();
  await db
    .prepare(
      "INSERT INTO assignments(id,user_id,role,company_id,modules_json,scope_kind,valid_from,valid_until) VALUES(?,?,'employee',?,'[]','holding',0,9999999999999)",
    )
    .bind(key(), f.claimant.userId, other.company)
    .run();
  expect(
    await readWorkCardPreferences(db, {
      ...other.claimant,
      userId: f.claimant.userId,
    }),
  ).toEqual({ pinnedIds: null, version: 0 });
  await expect(
    readWorkCardPreferences(db, { ...other.owner, userId: f.owner.userId }),
  ).rejects.toMatchObject({ status: 403 });
});
it("preserves existing hidden pins but refuses to add unavailable capabilities", async () => {
  await save(["counts", "expenses"], 0);
  const scoped = {
    ...f.claimant,
    branchId: f.branch,
    departmentId: f.department,
  };
  const allowed = async (): Promise<WorkCardId[]> => ["expenses", "services"];
  const result = await saveWorkCardPreferences(
    db,
    scoped,
    { pinnedIds: ["services", "counts", "expenses"], expectedVersion: 1 },
    key(),
    allowed,
  );
  expect(result.pinnedIds).toEqual(["services", "counts", "expenses"]);
  await expect(
    saveWorkCardPreferences(
      db,
      scoped,
      { pinnedIds: ["transfers"], expectedVersion: 2 },
      key(),
      allowed,
    ),
  ).rejects.toMatchObject({ code: "SHORTCUT_UNAVAILABLE" });
  expect((await readWorkCardPreferences(db, f.claimant)).version).toBe(2);
});
it("requires active access on reads and again in the committing transaction", async () => {
  let changed = false;
  const interrupted: SqlDatabase = {
    prepare: db.prepare.bind(db),
    exec: db.exec.bind(db),
    batch: async (statements: SqlStatement[]) => {
      if (!changed) {
        changed = true;
        await db
          .prepare("UPDATE assignments SET revoked_at=? WHERE user_id=?")
          .bind(Date.now(), f.claimant.userId)
          .run();
      }
      return db.batch(statements);
    },
  };
  await expect(
    saveWorkCardPreferences(
      interrupted,
      f.claimant,
      { pinnedIds: ["expenses"], expectedVersion: 0 },
      key(),
      available,
    ),
  ).rejects.toBeDefined();
  expect(
    await db
      .prepare(
        "SELECT 1 FROM work_card_preferences WHERE company_id=? AND user_id=?",
      )
      .bind(f.company, f.claimant.userId)
      .first(),
  ).toBeNull();
  await expect(readWorkCardPreferences(db, f.claimant)).rejects.toMatchObject({
    status: 403,
  });
});
it("requires selecting the warehouse grant and rejects department inventory creation", async () => {
  const missing = await f.member(
    "no-profile",
    "employee",
    1,
    f.department,
    false,
  );
  expect(
    (await getWorkCapabilities(db, missing)).creation.map((c) => c.kind),
  ).toEqual([
    "expense",
    "service_request",
    "equipment_request",
    "purchase_request",
    "leave",
  ]);
  const actor: WorkContext = {
    ...f.it,
    assignments: [
      {
        id: key(),
        role: "warehouse_manager",
        companyId: f.company,
        branchId: null,
        scopeKind: "holding",
        scopeNodeId: null,
        modules: [],
      },
    ],
  };
  expect((await getWorkCapabilities(db, actor)).creation.some(c=>c.kind==="stock_count")).toBe(false);
  actor.role="warehouse_manager";actor.assignmentId=actor.assignments[0].id;
  expect(
    (await getWorkCapabilities(db, actor)).creation.some(
      (c) => c.kind === "stock_count",
    ),
  ).toBe(true);
  expect(
    (
      await getWorkCapabilities(db, { ...actor, departmentId: f.department })
    ).creation.some((c) => c.kind === "stock_count"),
  ).toBe(false);
  const beta = await getWorkCapabilities(db, { ...actor, beta: true });
  expect(beta.creation.map((c) => c.kind)).toEqual([
    "expense",
    "service_request",
    "equipment_request",
    "purchase_request",
    "conversation",
  ]);
});
it("shows a team queue only for current coverage overlapping the workspace", async () => {
  const team = await saveServiceTeam(
    db,
    f.owner,
    null,
    serviceTeamSchema.parse({
      name: "Department support",
      members: [{ userId: f.it.userId, responsibility: "lead" }],
      coverage: [
        { category: "it", scopeKind: "department", scopeNodeId: f.department },
      ],
    }),
    key(),
  );
  expect(await hasCoveredTeam(db, f.it)).toBe(true);
  expect(
    await hasCoveredTeam(db, {
      ...f.it,
      branchId: f.branch,
      departmentId: f.department,
    }),
  ).toBe(true);
  expect(
    await hasCoveredTeam(db, {
      ...f.it,
      branchId: f.branch,
      departmentId: f.accountsDepartment,
    }),
  ).toBe(false);
  expect(await hasCoveredTeam(db, f.claimant)).toBe(false);
  await db
    .prepare("UPDATE service_teams SET active=0 WHERE id=?")
    .bind(team.id)
    .run();
  expect(await hasCoveredTeam(db, f.it)).toBe(false);
});
