import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import {
  expenseFixture,
  expenseDb as db,
  key,
} from "../../support/helpers/expense-fixture";
import {
  getWorkCapabilities,
  hasCoveredTeam,
} from "@erpx/server-work_projection/public/work-capabilities";
import {serviceTeamSchema} from '@erpx/server-support/public/service-teams';
import {seedServiceTeam} from '../../support/helpers/service-team-seed';
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

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
  const team = await seedServiceTeam(
    f.company,
    f.owner.userId,
    serviceTeamSchema.parse({
      name: "Department support",
      members: [{ userId: f.it.userId, responsibility: "lead" }],
      coverage: [
        { category: "it", scopeKind: "department", scopeNodeId: f.department },
      ],
    }),
    `team-${key()}`,
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
