import { beforeAll, expect, it, vi } from "vitest";
import { env, applyMigrations } from "./runtime-env";
import {
  expenseFixture,
  expenseDb as raw,
  key,
} from "./helpers/expense-fixture";
import {
  installStaffLevels,
  savePosition,
  positionSchema,
} from "../src/server/organization";
import {
  peopleLifecycleData,
  previewPackage,
  savePackage,
  bindPositionPackage,
  previewLifecycle,
  saveLifecycle,
  cancelLifecycle,
  requestLifecycleAccess,
  applyLifecycleReview,
  createRehireEngagement,
  peopleLifecycleWork,
} from "../src/server/people-lifecycle";
import { drainPeopleLifecycle } from "../src/server/people-lifecycle-jobs";
import {
  packageInput,
  lifecycleInput,
  peopleIamDestination,
} from "../src/lib/people-lifecycle";
import { iamDb } from "../src/server/iam-db";
import { iamActorFromWork } from "../src/server/iam-context";
import { loadIamAuthority } from "../src/server/iam-store";
import { selectWorkRole } from "../src/server/work-role-store";
import { decideIamReview } from "../src/server/iam-service";
import {
  managementCommand,
  readManagement,
} from "../src/server/user-management";
import { setDirectoryStatus } from "../src/server/people-directory";
import { transitionEngagement } from "../src/server/people-payroll";
import { companyMembers } from "../src/server/member-directory";
import { enrichFlowWork } from "../src/server/flow-work";
import type { WorkContext } from "../src/server/authorization";
vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: raw, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
vi.mock("../src/server/iam-mfa", () => ({ requireFreshMfa: async () => {} }));
const db = iamDb(raw);
beforeAll(async () => {
  await applyMigrations(raw, env.STAFF_TEST_MIGRATIONS);
  await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS);
});
async function authorize(a: WorkContext) {
  const sessionId = key();
  await selectWorkRole(raw, {
    sessionId,
    userId: a.userId,
    companyId: a.companyId,
    assignmentId: a.assignmentId,
    assignmentVersion: 1,
    revision: 0,
    expiresAt: Date.now() + 86400000,
  });
  return {
    ...a,
    authority: {
      sessionId,
      revision: 1,
      assignmentVersion: 1,
      expiresAt: Date.now() + 86400000,
    },
    iam: await loadIamAuthority(db, a.companyId, a.userId, a.role),
  } as WorkContext;
}
async function fixture() {
  const f = await expenseFixture(false),
    owner = await authorize(f.owner),
    it = await authorize(f.it),
    hr = await authorize(await f.member("hr", "hr_admin"));
  await installStaffLevels(raw, owner, key());
  const ceo = String(
    (
      await savePosition(
        raw,
        owner,
        null,
        positionSchema.parse({
          title: "Leadership",
          levelId: `org-level-${f.company}-1`,
          branchId: null,
          departmentId: null,
          reportsToPositionId: null,
          isCeo: true,
          expectedVersion: 0,
          reason: "Prepare leadership",
        }),
        key(),
      )
    ).id,
  );
  const input = positionSchema.parse({
    title: "Branch staff",
    requirements: "Branch printer and local access",
    levelId: `org-level-${f.company}-6`,
    branchId: f.branch,
    departmentId: f.department,
    reportsToPositionId: ceo,
    expectedVersion: 0,
    reason: "Prepare branch staff",
  });
  const command = key(),
    position = String((await savePosition(raw, hr, null, input, command)).id);
  await savePosition(raw, hr, null, input, command);
  await raw
    .prepare(
      "UPDATE hr_engagements SET position_id=?,level_id=? WHERE person_id=?",
    )
    .bind(ceo, `org-level-${f.company}-1`, `person-${owner.userId}`)
    .run();
  const person = `person-${f.claimant.userId}`,
    engagement = (await raw
      .prepare(
        "SELECT id FROM hr_engagements WHERE company_id=? AND person_id=?",
      )
      .bind(f.company, person)
      .first<{ id: string }>())!.id;
  return { ...f, owner, it, hr, ceo, position, person, engagement };
}
async function prepare(
  f: Awaited<ReturnType<typeof fixture>>,
  groupIds: string[] = [],
) {
  const role = (await raw
    .prepare(
      "SELECT published_revision FROM iam_roles WHERE company_id='*' AND id='employee'",
    )
    .first<{ published_revision: number }>())!;
  const input = packageInput.parse({
    expectedVersion: 0,
    reason: "Standard employee access",
    definition: {
      name: "Branch staff access",
      account: "local",
      groupIds,
      grants: [
        {
          roleId: "employee",
          expectedRoleRevision: role.published_revision,
          scopeKind: "holding",
          scopeNodeId: null,
          validUntil: Date.now() + 86400000,
        },
      ],
      tasks: [{ kind: "equipment", label: "Prepare laptop" }],
    },
  });
  const preview = await previewPackage(raw, f.it, input),
    command = key(),
    pkg = await savePackage(raw, f.it, input, command, true, preview.token);
  expect(
    await savePackage(raw, f.it, input, command, true, preview.token),
  ).toEqual(pkg);
  const binding = {
    positionId: f.position,
    packageId: pkg.id,
    revision: pkg.revision,
    expectedVersion: 1,
  };
  const result = await bindPositionPackage(raw, f.it, binding, key(), true);
  await bindPositionPackage(
    raw,
    f.it,
    { ...binding, previewToken: result.token },
    key(),
  );
  return { input, pkg };
}
it("creates one IT task on retried position creation and rename, without grants", async () => {
  const f = await fixture(),
    data = await peopleLifecycleData(raw, f.hr);
  expect(data.tasks.filter((t) => t.position_id === f.position)).toHaveLength(
    1,
  );
  expect(
    data.positions.find((p) => p.id === f.position)?.requirements,
  ).toContain("printer");
  const work = await peopleLifecycleWork(raw, f.it);
  expect(await enrichFlowWork(f.it, work)).toEqual(work);
  expect(
    work.some(
      (item) =>
        item.id === `position-prep-${f.position}` && item.queue === "todo",
    ),
  ).toBe(true);
  const count = await raw
    .prepare("SELECT count(*) n FROM assignments WHERE company_id=?")
    .bind(f.company)
    .first();
  await prepare(f);
  expect(
    await raw
      .prepare("SELECT count(*) n FROM assignments WHERE company_id=?")
      .bind(f.company)
      .first(),
  ).toEqual(count);
  expect(
    (await peopleLifecycleData(raw, f.it)).tasks.find(
      (t) => t.position_id === f.position,
    )?.status,
  ).toBe("completed");
  const rename = positionSchema.parse({
    title: "Renamed staff",
    requirements: "Branch printer and local access",
    levelId: `org-level-${f.company}-6`,
    branchId: f.branch,
    departmentId: f.department,
    reportsToPositionId: f.ceo,
    expectedVersion: 2,
    reason: "Change the display title",
  });
  await savePosition(raw, f.hr, f.position, rename, key());
  expect(
    (await peopleLifecycleData(raw, f.it)).tasks.find(
      (t) => t.position_id === f.position,
    )?.status,
  ).toBe("completed");
  await savePosition(
    raw,
    f.hr,
    f.position,
    {
      ...rename,
      expectedVersion: 3,
      requirements: "Additional finance application",
      reason: "Change work requirements",
    },
    key(),
  );
  const changed = (await peopleLifecycleData(raw, f.it)).tasks.filter(
    (t) => t.position_id === f.position,
  );
  expect(changed).toHaveLength(1);
  expect(changed[0].status).toBe("pending");
  const waiting = await previewLifecycle(
    raw,
    f.hr,
    lifecycleInput.parse({
      kind: "onboarding",
      expectedVersion: 0,
      personId: f.person,
      engagementId: f.engagement,
      positionId: f.position,
      managerPersonId: `person-${f.owner.userId}`,
      effectiveAt: Date.now() + 60000,
      reason: "Check changed work requirements",
    }),
  );
  expect(waiting.ready).toBe(false);
  expect(waiting.blockers.join(" ")).toContain("updated work requirements");
});
it("requires IT authority, published revisions, fresh previews, and Company ownership", async () => {
  const f = await fixture(),
    { input, pkg } = await prepare(f);
  await expect(
    savePackage(raw, f.hr, input, key(), false),
  ).rejects.toMatchObject({ status: 403 });
  const revision = { ...input, id: pkg.id, expectedVersion: pkg.version };
  const preview = await previewPackage(raw, f.it, revision);
  await raw
    .prepare("UPDATE organization_positions SET version=version+1 WHERE id=?")
    .bind(f.position)
    .run();
  await expect(
    savePackage(raw, f.it, revision, key(), true, preview.token),
  ).rejects.toMatchObject({ code: "PREVIEW_CHANGED" });
  await expect(
    bindPositionPackage(
      raw,
      f.it,
      {
        positionId: f.position,
        packageId: pkg.id,
        revision: 999,
        expectedVersion: 3,
      },
      key(),
      true,
    ),
  ).rejects.toMatchObject({ status: 409 });
});
it("keeps onboarding waiting for IT and identity verification without granting access", async () => {
  const f = await fixture();
  const input = lifecycleInput.parse({
    kind: "onboarding",
    expectedVersion: 0,
    personId: f.person,
    engagementId: f.engagement,
    positionId: f.position,
    managerPersonId: `person-${f.owner.userId}`,
    effectiveAt: Date.now() + 3600000,
    reason: "Onboard into branch",
  });
  const preview = await previewLifecycle(raw, f.hr, input);
  expect(preview.ready).toBe(false);
  expect(preview.blockers.join(" ")).toContain("IT");
  const command = key(),
    result = await saveLifecycle(
      raw,
      f.hr,
      input,
      command,
      true,
      preview.token,
    );
  expect(result.status).toBe("waiting_it");
  expect(
    await saveLifecycle(raw, f.hr, input, command, true, preview.token),
  ).toEqual(result);
});
it("enforces all-account Company cutoffs on time even before worker execution and supports cancellation", async () => {
  const f = await fixture(),
    alias = key(),
    otherCompany = key();
  await raw
    .prepare(
      "INSERT INTO iam_members(company_id,user_id,principal_id,person_id) VALUES(?,?,?,?),(?,?,?,?)",
    )
    .bind(
      f.company,
      f.claimant.userId,
      `local:${f.claimant.userId}`,
      f.person,
      f.company,
      alias,
      `local:${alias}`,
      f.person,
    )
    .run();
  const effectiveAt = Date.now() + 60000,
    input = lifecycleInput.parse({
      kind: "offboarding",
      expectedVersion: 0,
      personId: f.person,
      engagementId: f.engagement,
      positionId: null,
      effectiveAt,
      lastWorkingDate: "2026-09-16",
      reason: "End scoped engagement",
    });
  const preview = await previewLifecycle(raw, f.hr, input);
  expect(preview.accounts).toEqual(
    expect.arrayContaining([alias, f.claimant.userId]),
  );
  const result = await saveLifecycle(
    raw,
    f.hr,
    input,
    key(),
    true,
    preview.token,
  );
  for (const id of [alias, f.claimant.userId]) {
    expect(
      await raw
        .prepare("SELECT iam_principal_active(?,?,?) ok")
        .bind(id, f.company, effectiveAt - 1)
        .first("ok"),
    ).toBe(true);
    expect(
      await raw
        .prepare("SELECT iam_principal_active(?,?,?) ok")
        .bind(id, f.company, effectiveAt)
        .first("ok"),
    ).toBe(false);
    expect(
      await raw
        .prepare("SELECT iam_principal_active(?,?,?) ok")
        .bind(id, otherCompany, effectiveAt)
        .first("ok"),
    ).toBe(true);
  }
  await cancelLifecycle(raw, f.hr, String(result.id), 1, key());
  expect(
    await raw
      .prepare("SELECT iam_principal_active(?,?,?) ok")
      .bind(alias, f.company, effectiveAt)
      .first("ok"),
  ).toBe(true);
});
it("applies offboarding once, retains history, and never waits for directory tasks", async () => {
  const f = await fixture(),
    effectiveAt = Date.now() - 1000;
  const input = lifecycleInput.parse({
    kind: "offboarding",
    expectedVersion: 0,
    personId: f.person,
    engagementId: f.engagement,
    positionId: null,
    effectiveAt,
    lastWorkingDate: "2026-09-16",
    reason: "End employee access",
  });
  const preview = await previewLifecycle(raw, f.hr, input),
    result = await saveLifecycle(raw, f.hr, input, key(), true, preview.token);
  await expect(
    cancelLifecycle(raw, f.hr, String(result.id), 1, key()),
  ).rejects.toMatchObject({ code: "ALREADY_EFFECTIVE" });
  await drainPeopleLifecycle(raw);
  await drainPeopleLifecycle(raw);
  expect(
    await raw
      .prepare(
        "SELECT COUNT(*) n FROM outbox_events WHERE aggregate_id=? AND event_type='people.lifecycle.applied'",
      )
      .bind(result.id)
      .first("n"),
  ).toBe(1);
  expect(
    await raw
      .prepare("SELECT status FROM hr_engagements WHERE id=?")
      .bind(f.engagement)
      .first("status"),
  ).toBe("ended");
  expect(
    await raw
      .prepare("SELECT revoked_at FROM assignments WHERE id=?")
      .bind(f.claimant.assignmentId)
      .first("revoked_at"),
  ).toBe(effectiveAt);
  expect(
    (await peopleLifecycleData(raw, f.hr)).tasks.some(
      (t) => t.case_id === result.id && t.status === "pending",
    ),
  ).toBe(true);
});
it("requires independent personal reviews and does not grant before the effective time", async () => {
  const f = await fixture();
  await prepare(f);
  const input = lifecycleInput.parse({
    kind: "transfer",
    expectedVersion: 0,
    personId: f.person,
    engagementId: f.engagement,
    positionId: f.position,
    managerPersonId: `person-${f.owner.userId}`,
    effectiveAt: Date.now() + 60000,
    reason: "Transfer to prepared position",
  });
  const preview = await previewLifecycle(raw, f.hr, input);
  expect(preview.blockers).toEqual([]);
  const result = await saveLifecycle(
      raw,
      f.hr,
      input,
      key(),
      true,
      preview.token,
    ),
    requested = await requestLifecycleAccess(
      raw,
      f.hr,
      String(result.id),
      1,
      key(),
    );
  const reviewer = iamActorFromWork(f.owner);
  for (const id of requested.reviews)
    await decideIamReview(
      raw,
      reviewer,
      id,
      {
        expectedVersion: 1,
        decision: "approve",
        reason: "Independent review complete",
      },
      key(),
      (c, kind, p) => applyLifecycleReview(c, reviewer, kind, p),
    );
  const assignment = (await raw
    .prepare(
      "SELECT a.* FROM assignments a JOIN people_lifecycle_grants g ON a.id=g.assignment_id WHERE g.case_id=?",
    )
    .bind(result.id)
    .first<{ id: string; valid_from: number }>())!;
  expect(assignment.valid_from).toBe(input.effectiveAt);
  expect(
    await raw
      .prepare("SELECT iam_assignment_active(?,?) ok")
      .bind(assignment.id, input.effectiveAt - 1)
      .first("ok"),
  ).toBe(false);
  expect(
    await raw
      .prepare("SELECT iam_assignment_active(?,?) ok")
      .bind(assignment.id, input.effectiveAt)
      .first("ok"),
  ).toBe(true);
});
it("routes Company IAM to People while keeping advanced destinations explicit", () => {
  expect(peopleIamDestination("users")).toBe("people");
  expect(peopleIamDestination("groups")).toBe("technology?view=directory");
  expect(peopleIamDestination("roles")).toBe("people/settings?section=roles");
});

async function scheduledTransfer(
  f: Awaited<ReturnType<typeof fixture>>,
  effectiveAt = Date.now() - 1000,
) {
  const input = lifecycleInput.parse({
    kind: "transfer",
    expectedVersion: 0,
    personId: f.person,
    engagementId: f.engagement,
    positionId: f.position,
    managerPersonId: `person-${f.owner.userId}`,
    effectiveAt,
    reason: "Reviewed staff placement",
  });
  const preview = await previewLifecycle(raw, f.hr, input),
    result = await saveLifecycle(raw, f.hr, input, key(), true, preview.token);
  const requested = await requestLifecycleAccess(
      raw,
      f.hr,
      String(result.id),
      1,
      key(),
    ),
    reviewer = iamActorFromWork(f.owner);
  for (const id of requested.reviews)
    await decideIamReview(
      raw,
      reviewer,
      id,
      {
        expectedVersion: 1,
        decision: "approve",
        reason: "Independent reviewer approved",
      },
      key(),
      (c, kind, p) => applyLifecycleReview(c, reviewer, kind, p),
    );
  return String(result.id);
}
it("preserves independent access on transfer and rejects changed engagement snapshots", async () => {
  const f = await fixture();
  await prepare(f);
  const id = await scheduledTransfer(f);
  await drainPeopleLifecycle(raw);
  expect(
    await raw
      .prepare("SELECT status FROM people_lifecycle_cases WHERE id=?")
      .bind(id)
      .first("status"),
  ).toBe("completed");
  expect(
    await raw
      .prepare("SELECT revoked_at FROM assignments WHERE id=?")
      .bind(f.claimant.assignmentId)
      .first("revoked_at"),
  ).toBeNull();
  const second = await scheduledTransfer(f);
  const repeated = lifecycleInput.parse({
    kind: "onboarding",
    expectedVersion: 0,
    personId: f.person,
    engagementId: f.engagement,
    positionId: f.position,
    effectiveAt: Date.now() + 60000,
    reason: "Cannot onboard the same employment twice",
  });
  await expect(
    saveLifecycle(raw, f.hr, repeated, key(), false),
  ).rejects.toMatchObject({ code: "TRANSFER_REQUIRED" });
  await raw
    .prepare("UPDATE hr_engagements SET version=version+1 WHERE id=?")
    .bind(f.engagement)
    .run();
  const grant = (await raw
    .prepare(
      "SELECT assignment_id FROM people_lifecycle_grants WHERE case_id=?",
    )
    .bind(second)
    .first<{ assignment_id: string }>())!;
  expect(
    await raw
      .prepare("SELECT iam_assignment_active(?,?) ok")
      .bind(grant.assignment_id, Date.now())
      .first("ok"),
  ).toBe(false);
  await drainPeopleLifecycle(raw);
  expect(
    await raw
      .prepare("SELECT last_error FROM people_lifecycle_cases WHERE id=?")
      .bind(second)
      .first("last_error"),
  ).toBe("LIFECYCLE_REVIEW_CHANGED");
});
it("requires new engagement and new grants on rehire, without reviving old direct access", async () => {
  const f = await fixture();
  await prepare(f);
  const input = lifecycleInput.parse({
    kind: "offboarding",
    expectedVersion: 0,
    personId: f.person,
    engagementId: f.engagement,
    positionId: null,
    effectiveAt: Date.now() - 2000,
    lastWorkingDate: "2026-09-16",
    reason: "Complete previous employment",
  });
  await saveLifecycle(
    raw,
    f.hr,
    input,
    key(),
    true,
    (await previewLifecycle(raw, f.hr, input)).token,
  );
  await drainPeopleLifecycle(raw);
  const rehire = {
    personId: f.person,
    engagementId: f.engagement,
    positionId: f.position,
    startDate: "2026-09-16",
    reason: "Returning employee",
  };
  const command = key();
  const created = await createRehireEngagement(raw, f.hr, rehire, command);
  expect(await createRehireEngagement(raw, f.hr, rehire, command)).toEqual(
    created,
  );
  await expect(
    createRehireEngagement(raw, f.hr, rehire, key()),
  ).rejects.toThrow();
  const hired = { ...f, engagement: String(created.id) };
  const id = await scheduledTransfer(hired, Date.now() - 500);
  await drainPeopleLifecycle(raw);
  expect(
    await raw
      .prepare("SELECT status FROM people_lifecycle_cases WHERE id=?")
      .bind(id)
      .first("status"),
  ).toBe("completed");
  expect(
    await raw
      .prepare("SELECT iam_assignment_active(?,?) ok")
      .bind(f.claimant.assignmentId, Date.now())
      .first("ok"),
  ).toBe(false);
  const grant = (await raw
    .prepare(
      "SELECT assignment_id FROM people_lifecycle_grants WHERE case_id=?",
    )
    .bind(id)
    .first<{ assignment_id: string }>())!;
  expect(
    await raw
      .prepare("SELECT iam_assignment_active(?,?) ok")
      .bind(grant.assignment_id, Date.now())
      .first("ok"),
  ).toBe(true);
  expect(
    await raw
      .prepare("SELECT status FROM hr_engagements WHERE id=?")
      .bind(f.engagement)
      .first("status"),
  ).toBe("ended");
});
it("does not let one failed placement delay another Company's departure", async () => {
  const f = await fixture();
  await prepare(f);
  const blocked = await scheduledTransfer(f);
  await raw
    .prepare("UPDATE hr_engagements SET version=version+1 WHERE id=?")
    .bind(f.engagement)
    .run();
  const other = await fixture(),
    input = lifecycleInput.parse({
      kind: "offboarding",
      expectedVersion: 0,
      personId: other.person,
      engagementId: other.engagement,
      positionId: null,
      effectiveAt: Date.now() - 500,
      lastWorkingDate: "2026-09-16",
      reason: "Independent departure",
    });
  const departure = await saveLifecycle(
    raw,
    other.hr,
    input,
    key(),
    true,
    (await previewLifecycle(raw, other.hr, input)).token,
  );
  await drainPeopleLifecycle(raw);
  expect(
    await raw
      .prepare("SELECT status FROM people_lifecycle_cases WHERE id=?")
      .bind(blocked)
      .first("status"),
  ).toBe("scheduled");
  expect(
    await raw
      .prepare("SELECT status FROM people_lifecycle_cases WHERE id=?")
      .bind(departure.id)
      .first("status"),
  ).toBe("completed");
});
it("blocks protected administrator promotion after a departure was scheduled", async () => {
  const f = await fixture(),
    input = lifecycleInput.parse({
      kind: "offboarding",
      expectedVersion: 0,
      personId: f.person,
      engagementId: f.engagement,
      positionId: null,
      effectiveAt: Date.now() + 60000,
      lastWorkingDate: "2026-09-16",
      reason: "Schedule departure",
    });
  const result = await saveLifecycle(
    raw,
    f.hr,
    input,
    key(),
    true,
    (await previewLifecycle(raw, f.hr, input)).token,
  );
  await expect(
    raw
      .prepare(
        "UPDATE assignments SET role='holding_owner',scope_kind='holding',scope_node_id=NULL WHERE id=?",
      )
      .bind(f.claimant.assignmentId)
      .run(),
  ).rejects.toThrow();
  const recovery = await f.member(
    "protected-unlinked",
    "holding_owner",
    1,
    f.department,
    false,
  );
  await expect(
    raw
      .prepare("UPDATE hr_people SET user_id=? WHERE id=?")
      .bind(recovery.userId, f.person)
      .run(),
  ).rejects.toMatchObject({ code: "23514" });
  const command = key();
  const cancelled = await cancelLifecycle(
    raw,
    f.hr,
    String(result.id),
    1,
    command,
  );
  expect(
    await cancelLifecycle(raw, f.hr, String(result.id), 1, command),
  ).toEqual(cancelled);
});

it("allows IT to prepare groups without granting access-policy authority", async () => {
  const f = await fixture(),
    actor = iamActorFromWork(f.it);
  const change = {
    operation: "group" as const,
    input: {
      name: "IT prepared",
      kind: "manual" as const,
      parentId: null,
      scopeNodeId: null,
      rule: null,
      archived: false,
      expectedVersion: 0,
      reason: "Prepare reusable access group",
    },
  };
  const preview = await managementCommand(raw, actor, change, key(), true);
  const group = await managementCommand(
    raw,
    actor,
    change,
    key(),
    false,
    preview.previewToken,
  );
  expect(
    (await readManagement(raw, actor)).groups.some((g) => g.id === group.id),
  ).toBe(true);
  await expect(
    managementCommand(
      raw,
      actor,
      {
        operation: "policy",
        input: {
          name: "Not delegated",
          archived: false,
          expectedVersion: 0,
          reason: "Cannot grant access",
          grants: [
            {
              roleId: "employee",
              expectedRoleRevision: 2,
              scopeKind: "holding",
              scopeNodeId: null,
              validUntil: Date.now() + 60000,
            },
          ],
        },
      },
      key(),
      true,
    ),
  ).rejects.toMatchObject({ status: 403 });
});

it("excludes superseded accounts from new grants but retains them in departure coverage", async () => {
  const f = await fixture();
  await prepare(f);
  const alias = key();
  await raw
    .prepare(
      "INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status) VALUES(?,?,?,?,'superseded')",
    )
    .bind(f.company, alias, `local:${alias}`, f.person)
    .run();
  const input = lifecycleInput.parse({
    kind: "transfer",
    expectedVersion: 0,
    personId: f.person,
    engagementId: f.engagement,
    positionId: f.position,
    managerPersonId: `person-${f.owner.userId}`,
    effectiveAt: Date.now() + 60000,
    reason: "Use the verified current account",
  });
  expect((await previewLifecycle(raw, f.hr, input)).accounts).toEqual([
    f.claimant.userId,
  ]);
  expect(
    (
      await previewLifecycle(raw, f.hr, {
        ...input,
        kind: "offboarding",
        positionId: null,
      })
    ).accounts,
  ).toContain(alias);
  await scheduledTransfer(f);
});

it("closes rejected individual reviews without activating partial grants", async () => {
  const f = await fixture();
  await prepare(f);
  const input = lifecycleInput.parse({
    kind: "transfer",
    expectedVersion: 0,
    personId: f.person,
    engagementId: f.engagement,
    positionId: f.position,
    managerPersonId: `person-${f.owner.userId}`,
    effectiveAt: Date.now() + 60000,
    reason: "Review access independently",
  });
  const preview = await previewLifecycle(raw, f.hr, input);
  const result = await saveLifecycle(
    raw,
    f.hr,
    input,
    key(),
    true,
    preview.token,
  );
  const requested = await requestLifecycleAccess(
    raw,
    f.hr,
    String(result.id),
    1,
    key(),
  );
  const reviewer = iamActorFromWork(f.owner);
  await decideIamReview(
    raw,
    reviewer,
    requested.reviews[0],
    {
      decision: "reject",
      expectedVersion: 1,
      reason: "Revise the requested access",
    },
    key(),
    undefined,
    (c, kind, p) => applyLifecycleReview(c, reviewer, kind, p, "reject"),
  );
  expect(
    await raw
      .prepare("SELECT status FROM people_lifecycle_cases WHERE id=?")
      .bind(result.id)
      .first("status"),
  ).toBe("blocked");
  expect(
    await raw
      .prepare("SELECT people_case_active(?,?) ok")
      .bind(result.id, input.effectiveAt)
      .first("ok"),
  ).toBe(false);
  await cancelLifecycle(raw, f.hr, String(result.id), 3, key());
});

it("prevents legacy status controls from bypassing lifecycle cutoff or ending history early", async () => {
  const f = await fixture();
  await expect(
    transitionEngagement(
      f.hr,
      f.engagement,
      { status: "ended", expectedVersion: 1, endDate: "2026-09-16" },
      key(),
    ),
  ).rejects.toMatchObject({ code: "LIFECYCLE_REQUIRED" });
  await expect(
    setDirectoryStatus(
      f.hr,
      f.person,
      { status: "inactive", expectedVersion: 1 },
      key(),
    ),
  ).rejects.toMatchObject({ code: "LIFECYCLE_REQUIRED" });
  await expect(
    setDirectoryStatus(
      f.hr,
      f.person,
      { status: "archived", expectedVersion: 1 },
      key(),
    ),
  ).rejects.toMatchObject({ code: "ENGAGEMENT_NOT_ENDED" });
});

it("shares immutable packages across positions and preserves another occupant's membership after departure", async () => {
  const f = await fixture(),
    itActor = iamActorFromWork(f.it);
  const change = {
    operation: "group" as const,
    input: {
      name: "Shared local group",
      kind: "manual" as const,
      parentId: null,
      scopeNodeId: null,
      rule: null,
      archived: false,
      expectedVersion: 0,
      reason: "Prepare a shared position group",
    },
  };
  const groupPreview = await managementCommand(
    raw,
    itActor,
    change,
    key(),
    true,
  );
  const group = await managementCommand(
    raw,
    itActor,
    change,
    key(),
    false,
    groupPreview.previewToken,
  );
  const { pkg, input: original } = await prepare(f, [group.id]);
  const secondPosition = String(
    (
      await savePosition(
        raw,
        f.hr,
        null,
        positionSchema.parse({
          title: "Shared package position",
          requirements: "Same tools",
          levelId: `org-level-${f.company}-6`,
          branchId: f.branch,
          departmentId: f.department,
          reportsToPositionId: f.ceo,
          expectedVersion: 0,
          reason: "Reuse the approved package",
        }),
        key(),
      )
    ).id,
  );
  const binding = {
    positionId: secondPosition,
    packageId: pkg.id,
    revision: pkg.revision,
    expectedVersion: 1,
  };
  const bindingPreview = await bindPositionPackage(
    raw,
    f.it,
    binding,
    key(),
    true,
  );
  await bindPositionPackage(
    raw,
    f.it,
    { ...binding, previewToken: bindingPreview.token },
    key(),
  );
  await scheduledTransfer(f);
  await drainPeopleLifecycle(raw);
  const peer = await f.member("Shared colleague", "employee");
  const second = {
    ...f,
    person: `person-${peer.userId}`,
    engagement: `eng-${peer.userId}`,
    position: secondPosition,
  };
  await scheduledTransfer(second);
  await drainPeopleLifecycle(raw);
  const isMember = async (id: string) =>
    raw
      .prepare("SELECT um_group_member(?,?,?) ok")
      .bind(f.company, id, group.id)
      .first("ok");
  expect(await isMember(f.person)).toBe(true);
  expect(await isMember(second.person)).toBe(true);
  const edit = {
    ...original,
    id: pkg.id,
    expectedVersion: pkg.version,
    definition: { ...original.definition, groupIds: [] },
  };
  const impact = await previewPackage(raw, f.it, edit);
  expect(impact.affectedPositions).toHaveLength(2);
  expect(impact.affectedPeople).toHaveLength(2);
  await savePackage(raw, f.it, edit, key(), true, impact.token);
  // Existing personal reviews still refer to their immutable revision.
  expect(await isMember(f.person)).toBe(true);
  const off = lifecycleInput.parse({
    kind: "offboarding",
    expectedVersion: 0,
    personId: f.person,
    engagementId: f.engagement,
    positionId: null,
    effectiveAt: Date.now() - 1,
    lastWorkingDate: "2026-09-16",
    reason: "End one shared-package occupant",
  });
  await saveLifecycle(
    raw,
    f.hr,
    off,
    key(),
    true,
    (await previewLifecycle(raw, f.hr, off)).token,
  );
  await drainPeopleLifecycle(raw);
  expect(await isMember(f.person)).toBe(false);
  expect(await isMember(second.person)).toBe(true);
  expect(
    (await readManagement(raw, itActor)).groups.find((g) => g.id === group.id)
      ?.memberSources?.[second.person],
  ).toContain("Position access package");
});

it("groups accounts by reviewed identity links rather than matching email", async () => {
  const f = await fixture(),
    alias = await f.member(
      "identity-alias",
      "employee",
      1,
      f.department,
      false,
    ),
    email = `${key()}@example.test`;
  await env.CONTROL_DB.prepare(
    'INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?)',
  )
    .bind(alias.userId, "Alias account", email, Date.now(), Date.now())
    .run();
  await raw
    .prepare("UPDATE hr_people SET user_id=NULL,email=? WHERE id=?")
    .bind(email, f.person)
    .run();
  expect(
    (await companyMembers(f.hr)).find((m) => m.id === alias.userId)?.personId,
  ).toBeNull();
  await raw
    .prepare(
      "INSERT INTO iam_members(company_id,user_id,principal_id,person_id) VALUES(?,?,?,?)",
    )
    .bind(f.company, alias.userId, `local:${alias.userId}`, f.person)
    .run();
  expect(
    (await companyMembers(f.hr)).find((m) => m.id === alias.userId)?.personId,
  ).toBe(f.person);
});

it("creates six retirement responsibilities, keeps cutoff independent and prevents IT from closing finance", async () => {
 const {calendarCommand}=await import("../src/server/calendar/commands");
 const {retirementWork}=await import("../src/server/retirement-clearance");
 const f=await fixture(),finance=await authorize(await f.member("clearance-finance","finance_preparer")),now=Date.now();
 const input=lifecycleInput.parse({kind:"offboarding",departureReason:"retirement",expectedVersion:0,personId:f.person,engagementId:f.engagement,positionId:null,lastWorkingDate:"2026-09-16",effectiveAt:now-1000,reason:"Fictional retirement",clearanceOwners:{finance:finance.userId}});
 const preview=await previewLifecycle(raw,f.hr,input),command=key();
 const saved=await saveLifecycle(raw,f.hr,input,command,true,preview.token);
 expect(await saveLifecycle(raw,f.hr,input,command,true,preview.token)).toEqual(saved);
 const tasks=(await peopleLifecycleData(raw,f.hr)).tasks.filter(t=>t.case_id===saved.id);
 expect(tasks).toHaveLength(6);
 expect(tasks.find(t=>t.clearance_type==="hr")?.assignee_id).toBe(f.hr.userId);
 expect(tasks.find(t=>t.clearance_type==="payroll")?.assignee_id).toBeNull();
 expect(await raw.prepare("SELECT iam_assignment_active(?,?) ok").bind(f.claimant.assignmentId,now).first("ok")).toBe(false);
 const financial=tasks.find(t=>t.clearance_type==="finance")!;
 await expect(calendarCommand(raw,f.it,{action:"task.complete",id:financial.id,expectedVersion:1,status:"not_required",reason:"IT cannot close this"},key())).rejects.toMatchObject({status:403});
 await expect(calendarCommand(raw,finance,{action:"task.complete",id:financial.id,expectedVersion:1},key())).rejects.toMatchObject({status:422});
 await drainPeopleLifecycle(raw);
 expect((await retirementWork(raw,finance)).some(t=>t.id===financial.id&&t.canAct)).toBe(true);
 await calendarCommand(raw,finance,{action:"task.complete",id:financial.id,expectedVersion:1,status:"not_required",reason:"Reviewed: no outstanding financial documents"},key());
 expect(await raw.prepare("SELECT status FROM people_preparation_tasks WHERE id=?").bind(financial.id).first("status")).toBe("not_required");
 expect(await raw.prepare("SELECT end_date FROM hr_engagements WHERE id=?").bind(f.engagement).first("end_date")).toBe("2026-09-16");
});
it("retirement cancellation keeps history and cancels pending clearance before cutoff",async()=>{
 const f=await fixture();
 const input=lifecycleInput.parse({kind:"offboarding",departureReason:"retirement",expectedVersion:0,personId:f.person,engagementId:f.engagement,positionId:null,lastWorkingDate:"2026-12-31",effectiveAt:Date.now()+86400000,reason:"Retirement review"});
 const saved=await saveLifecycle(raw,f.hr,input,key(),true,(await previewLifecycle(raw,f.hr,input)).token);
 await cancelLifecycle(raw,f.hr,String(saved.id),Number(saved.version),key());
 expect(await raw.prepare("SELECT count(*) n FROM people_preparation_tasks WHERE case_id=? AND status='cancelled'").bind(saved.id).first("n")).toBe(6);
 expect(await raw.prepare("SELECT iam_assignment_active(?,?) ok").bind(f.claimant.assignmentId,Date.now()+86400001).first("ok")).toBe(true);
});

it("reads retirement evidence through source permissions and never changes the financial document",async()=>{
 const {calendarCommand}=await import("../src/server/calendar/commands");
 const {clearanceCase}=await import("../src/server/retirement-clearance");
 const f=await fixture(),finance=await authorize(await f.member("clearance-evidence","finance_preparer")),expenseId=await f.claim(finance);
 const before=await raw.prepare("SELECT status,version FROM finance_expenses WHERE id=?").bind(expenseId).first();
 const input=lifecycleInput.parse({kind:"offboarding",departureReason:"retirement",expectedVersion:0,personId:f.person,engagementId:f.engagement,positionId:null,lastWorkingDate:"2026-12-31",effectiveAt:Date.now()+86400000,reason:"Retirement evidence review",clearanceOwners:{finance:finance.userId}});
 const saved=await saveLifecycle(raw,f.hr,input,key(),true,(await previewLifecycle(raw,f.hr,input)).token),task=(await peopleLifecycleData(raw,f.hr)).tasks.find(t=>t.case_id===saved.id&&t.clearance_type==="finance")!;
 await expect(calendarCommand(raw,finance,{action:"task.complete",id:task.id,expectedVersion:1,status:"completed",reason:"Review without source"},key())).rejects.toMatchObject({status:422});
 const command={action:"task.complete",id:task.id,expectedVersion:1,status:"completed",reason:"Reviewed source ledger; no payment performed",references:[{kind:"expense",id:expenseId}]};
 const receipt=key();await calendarCommand(raw,finance,command,receipt);await calendarCommand(raw,finance,command,receipt);
 await expect(calendarCommand(raw,finance,command,key())).rejects.toMatchObject({status:409});
 expect(await raw.prepare("SELECT status,version FROM finance_expenses WHERE id=?").bind(expenseId).first()).toEqual(before);
 expect((await clearanceCase(raw,finance,String(saved.id))).tasks.find(t=>t.id===task.id)!.evidence[0].status).toBe("draft");
 const hrEvidence=(await clearanceCase(raw,f.hr,String(saved.id))).tasks.find(t=>t.id===task.id)!.evidence[0];
 expect(hrEvidence.href).toBeNull();
 expect((await clearanceCase(raw,f.hr,String(saved.id))).tasks.find(t=>t.id===task.id)!.completion_reason).toBeNull();
 expect(JSON.stringify(await peopleLifecycleData(raw,f.hr))).not.toContain(expenseId);
 const other=await fixture();await expect(clearanceCase(raw,other.hr,String(saved.id))).rejects.toMatchObject({status:404});
});
it("revalidates clearance owners, rejects stale edits and keeps reassignment notifications attached to the same task",async()=>{
 const {calendarCommand}=await import("../src/server/calendar/commands");
 const {clearanceOwners}=await import("../src/server/retirement-clearance");
 const f=await fixture(),finance=await authorize(await f.member("clearance-revocation","finance_preparer"));
 const input=lifecycleInput.parse({kind:"offboarding",departureReason:"retirement",expectedVersion:0,personId:f.person,engagementId:f.engagement,positionId:null,lastWorkingDate:"2026-12-31",effectiveAt:Date.now()+86400000,reason:"Retirement assignment review"});
 const saved=await saveLifecycle(raw,f.hr,input,key(),true,(await previewLifecycle(raw,f.hr,input)).token),task=(await peopleLifecycleData(raw,f.hr)).tasks.find(t=>t.case_id===saved.id&&t.clearance_type==="finance")!;
 expect((await clearanceOwners(raw,f.hr)).find(o=>o.id===f.it.userId)?.kinds).not.toContain("finance");
 const plan={action:"task.plan",id:task.id,expectedVersion:1,assigneeId:finance.userId,dueDate:"2026-12-31",responsibility:"finance"};
 await calendarCommand(raw,f.hr,plan,key());
 await expect(calendarCommand(raw,f.hr,plan,key())).rejects.toMatchObject({status:409});
 expect(await raw.prepare("SELECT count(*) n FROM staff_notifications WHERE record_type='retirement_clearance' AND record_id=? AND user_id=?").bind(task.id,finance.userId).first("n")).toBe(1);
 await raw.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(),finance.assignmentId).run();
 await expect(calendarCommand(raw,finance,{action:"task.complete",id:task.id,expectedVersion:2,status:"not_required",reason:"Authority was withdrawn"},key())).rejects.toMatchObject({status:403});
 expect(await raw.prepare("SELECT status FROM people_preparation_tasks WHERE id=?").bind(task.id).first("status")).toBe("pending");
});

it("paginates the directory in SQL and resolves details outside the current page",async()=>{
 const {peopleDirectoryPage}=await import("../src/server/people-directory-page");
 const f=await fixture();const prefix=key();
 await raw.batch(Array.from({length:65},(_,i)=>raw.prepare("INSERT INTO hr_people(id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES(?,?,?,'TH',?,1,1)").bind(`${prefix}-${i}`,f.company,`Pagination ${String(i).padStart(3,'0')}`,f.hr.userId)));
 const first=await peopleDirectoryPage(f.hr,{q:"Pagination"});
 expect(first.items).toHaveLength(30);expect(Array.isArray(first.items[0].accounts)).toBe(true);
 const second=await peopleDirectoryPage(f.hr,{q:"Pagination",cursor:first.nextCursor!});
 expect(second.items).toHaveLength(30);expect(new Set([...first.items,...second.items].map(r=>r.id)).size).toBe(60);
 const detail=await peopleDirectoryPage(f.hr,{q:"Pagination",person:`${prefix}-64`});
 expect(detail.selected?.id).toBe(`${prefix}-64`);expect(detail.items.some(r=>r.id===detail.selected?.id)).toBe(false);
 const search=await peopleDirectoryPage(f.hr,{q:"Pagination 064"});expect(search.items.map(r=>r.id)).toEqual([`${prefix}-64`]);
 const other=await peopleDirectoryPage({...f.hr,companyId:key()},{q:"Pagination"});expect(other.items).toEqual([]);
});

it("HR placement works without a package, linked account or IT preparation and never grants access", async()=>{
  const f=await fixture();
  await raw.prepare("UPDATE hr_people SET user_id=NULL WHERE id=? AND company_id=?").bind(f.person,f.company).run();
  const input=lifecycleInput.parse({expectedVersion:0,kind:'transfer',personId:f.person,engagementId:f.engagement,positionId:f.position,managerPersonId:`person-${f.owner.userId}`,effectiveAt:Date.now()+20,reason:'HR-first placement'});
  const count=await raw.prepare("SELECT count(*) n FROM assignments WHERE company_id=?").bind(f.company).first();
  const preview=await previewLifecycle(raw,f.hr,input,true);
  expect(preview.ready).toBe(true);expect(preview.added).toEqual([]);expect(preview.removed).toEqual([]);
  const result=await saveLifecycle(raw,f.hr,input,key(),true,preview.token,true);
  expect(result.status).toBe('scheduled');
  await drainPeopleLifecycle(raw,Date.now()+50);
  expect((await raw.prepare("SELECT position_id,status FROM hr_engagements WHERE id=?").bind(f.engagement).first())).toMatchObject({position_id:f.position,status:'active'});
  expect(await raw.prepare("SELECT count(*) n FROM assignments WHERE company_id=?").bind(f.company).first()).toEqual(count);
  expect((await peopleLifecycleData(raw,f.hr)).roster.find(p=>p.id===f.person)?.itStatus).toBe('waiting');
});

it("IT applies pinned resources and requests independently reviewed ERPX roles, retaining them across HR transfers",async()=>{
  const {previewProvisioning,applyProvisioning,drainProvisioning,applyProvisioningReview,technologyData}=await import('../src/server/technology');
  const {provisioningInput}=await import('../src/lib/technology');
  const f=await fixture();const {pkg}=await prepare(f);
  await raw.prepare("UPDATE hr_engagements SET position_id=? WHERE id=?").bind(f.position,f.engagement).run();
  const input=provisioningInput.parse({engagementIds:[f.engagement],validUntil:Date.now()+3600000,reason:'IT-first apply pinned revision'}),actor=iamActorFromWork(f.it);
  const preview=await previewProvisioning(db,actor,input),command=key();
  const result=await applyProvisioning(db,actor,input,preview.token,command);
  expect(await applyProvisioning(db,actor,input,preview.token,command)).toEqual(result);
  expect(result.reviews).toHaveLength(1);
  await drainProvisioning(db);
  expect((await technologyData(db,actor)).operations.find(o=>o.id===result.id)?.status).toBe('completed');
  expect((await db.pool.query("SELECT count(*)::int n FROM assignments WHERE company_id=$1 AND source='provisioning'",[f.company])).rows[0].n).toBe(0);
  await expect(decideIamReview(db,actor,result.reviews[0],{decision:'approve',expectedVersion:1,reason:'Self approval denied'},key(),(c,k,p)=>applyProvisioningReview(c,actor,p))).rejects.toMatchObject({status:403});
  const reviewer=iamActorFromWork(f.owner);
  await decideIamReview(db,reviewer,result.reviews[0],{decision:'approve',expectedVersion:1,reason:'Independent role review'},key(),(c,k,p)=>applyProvisioningReview(c,reviewer,p));
  const assignment=(await db.pool.query("SELECT id FROM assignments WHERE company_id=$1 AND source='provisioning'",[f.company])).rows[0].id;
  expect((await db.pool.query("SELECT iam_assignment_active($1,$2) ok",[assignment,Date.now()])).rows[0].ok).toBe(true);
  const movedPosition=String((await savePosition(raw,f.hr,null,positionSchema.parse({title:'Changed HR position',levelId:`org-level-${f.company}-6`,branchId:f.branch,departmentId:f.department,reportsToPositionId:f.ceo,expectedVersion:0,reason:'HR move'}),key())).id);
  const placement=lifecycleInput.parse({expectedVersion:0,kind:'transfer',personId:f.person,engagementId:f.engagement,positionId:movedPosition,managerPersonId:`person-${f.owner.userId}`,effectiveAt:Date.now()+10,reason:'HR move without new access'});
  const next=await previewLifecycle(raw,f.hr,placement,true);await saveLifecycle(raw,f.hr,placement,key(),true,next.token,true);await drainPeopleLifecycle(raw,Date.now()+50);
  expect((await db.pool.query("SELECT iam_assignment_active($1,$2) ok",[assignment,Date.now()+60])).rows[0].ok).toBe(true);
  expect((await technologyData(db,actor)).applications.find(a=>a.id.startsWith(result.id))?.package_revision).toBe(pkg.revision);
  expect((await db.pool.query("SELECT iam_assignment_active($1,$2) ok",[assignment,input.validUntil+1])).rows[0].ok).toBe(false);
});

it("keeps supplemental access independently and withdraws only its selected source",async()=>{
 const {previewProvisioning,applyProvisioning,drainProvisioning,technologyData}=await import('../src/server/technology');
 const {provisioningInput}=await import('../src/lib/technology');
 const f=await fixture(),{pkg}=await prepare(f),actor=iamActorFromWork(f.it);
 await db.pool.query('UPDATE hr_engagements SET position_id=$2 WHERE id=$1',[f.engagement,f.position]);
 const request={engagementIds:[f.engagement],validUntil:Date.now()+3600000,exclude:['role:employee'],reason:'Verify independent package source'};
 async function apply(value:unknown){const input=provisioningInput.parse(value);const p=await previewProvisioning(db,actor,input);const result=await applyProvisioning(db,actor,input,p.token,key());await drainProvisioning(db);return result;}
 const first=await apply(request);
 const supplemental=await apply({...request,sourceKind:'supplemental',packageId:pkg.id,revision:pkg.revision});
 const replacement=await apply(request);
 let data=await technologyData(db,actor);
 expect(data.applications.filter(a=>a.active).map(a=>a.id.split(':')[0]).sort()).toEqual([supplemental.id,replacement.id].sort());
 expect(data.applications.find(a=>a.id.startsWith(first.id))?.ended_at).toBeTruthy();
 const target=data.applications.find(a=>a.id.startsWith(supplemental.id))!;
 await apply({...request,packageId:pkg.id,revision:pkg.revision,revokeOnly:true,revokeApplicationIds:[target.id]});
 data=await technologyData(db,actor);
 expect(data.applications.filter(a=>a.active).map(a=>a.id.split(':')[0])).toEqual([replacement.id]);
 await expect(db.pool.query("UPDATE people_applied_packages SET definition='{}' WHERE id=$1",[target.id])).rejects.toThrow('immutable');
});

it("revalidates HR position scope before applying a scheduled placement",async()=>{
 const f=await fixture();
 const input=lifecycleInput.parse({expectedVersion:0,kind:'transfer',personId:f.person,engagementId:f.engagement,positionId:f.position,managerPersonId:`person-${f.owner.userId}`,effectiveAt:Date.now()+100,reason:'Scheduled HR placement'});
 const p=await previewLifecycle(raw,f.hr,input,true),result=await saveLifecycle(raw,f.hr,input,key(),true,p.token,true);
 await db.pool.query('UPDATE organization_positions SET department_id=NULL,version=version+1 WHERE id=$1',[f.position]);
 await drainPeopleLifecycle(raw,Date.now()+200);
 expect((await db.pool.query('SELECT status,last_error FROM people_lifecycle_cases WHERE id=$1',[result.id])).rows[0]).toMatchObject({last_error:'LIFECYCLE_PLACEMENT_CHANGED'});
});

it("provisions a shared package for multiple positions only when IT selects their employees",async()=>{
 const {previewProvisioning,applyProvisioning,drainProvisioning,technologyData}=await import('../src/server/technology');
 const {provisioningInput}=await import('../src/lib/technology');
 const f=await fixture(),{pkg}=await prepare(f),actor=iamActorFromWork(f.it),other=await f.member('bulk','employee');
 const second=String((await savePosition(raw,f.hr,null,positionSchema.parse({title:'Second package consumer',levelId:`org-level-${f.company}-6`,branchId:f.branch,departmentId:f.department,reportsToPositionId:f.ceo,expectedVersion:0,reason:'Share reusable IT preparation'}),key())).id);
 const binding={positionId:second,packageId:pkg.id,revision:pkg.revision,expectedVersion:1},p=await bindPositionPackage(raw,f.it,binding,key(),true);
 await bindPositionPackage(raw,f.it,{...binding,previewToken:p.token},key());
 const otherEngagement=(await db.pool.query('SELECT id FROM hr_engagements WHERE company_id=$1 AND person_id=$2',[f.company,`person-${other.userId}`])).rows[0].id;
 await db.pool.query('UPDATE hr_engagements SET position_id=$2 WHERE id=$1',[f.engagement,f.position]);
 await db.pool.query('UPDATE hr_engagements SET position_id=$2 WHERE id=$1',[otherEngagement,second]);
 expect((await technologyData(db,actor)).applications).toHaveLength(0);
 const input=provisioningInput.parse({engagementIds:[f.engagement,otherEngagement],validUntil:Date.now()+3600000,exclude:['role:employee'],reason:'Apply both selected employees'}),preview=await previewProvisioning(db,actor,input);
 expect(preview.employees).toHaveLength(2);
 const result=await applyProvisioning(db,actor,input,preview.token,key());await drainProvisioning(db);
 expect((await technologyData(db,actor)).applications.filter(a=>a.active&&a.id.startsWith(result.id))).toHaveLength(2);
});

it("keeps cutoffs effective after HR returns an employee and requires new IT access",async()=>{
 const {previewProvisioning,applyProvisioning,drainProvisioning,technologyData}=await import('../src/server/technology');
 const {provisioningInput}=await import('../src/lib/technology');
 const f=await fixture();await prepare(f);const actor=iamActorFromWork(f.it);
 await db.pool.query('UPDATE hr_engagements SET position_id=$2 WHERE id=$1',[f.engagement,f.position]);
 const input=provisioningInput.parse({engagementIds:[f.engagement],validUntil:Date.now()+3600000,exclude:['role:employee'],reason:'Initial resources'});
 let preview=await previewProvisioning(db,actor,input);const first=await applyProvisioning(db,actor,input,preview.token,key());await drainProvisioning(db);
 const cutoff=lifecycleInput.parse({expectedVersion:0,kind:'suspend',personId:f.person,engagementId:f.engagement,positionId:null,effectiveAt:Date.now(),reason:'Suspend company access'});
 const cp=await previewLifecycle(raw,f.hr,cutoff,true);await saveLifecycle(raw,f.hr,cutoff,key(),true,cp.token,true);await drainPeopleLifecycle(raw);
 expect((await technologyData(db,actor)).applications.every(a=>!a.active)).toBe(true);
 const returned=lifecycleInput.parse({expectedVersion:0,kind:'transfer',personId:f.person,engagementId:f.engagement,positionId:f.position,managerPersonId:`person-${f.owner.userId}`,effectiveAt:Date.now()+1,reason:'HR confirms return'});
 const rp=await previewLifecycle(raw,f.hr,returned,true);await saveLifecycle(raw,f.hr,returned,key(),true,rp.token,true);await drainPeopleLifecycle(raw,Date.now()+10);
 await expect(previewProvisioning(db,actor,input)).rejects.toMatchObject({code:'ACCESS_CUTOFF_ACTIVE'});
 const restore={...input,restoreAccess:true};preview=await previewProvisioning(db,actor,restore);const restored=await applyProvisioning(db,actor,restore,preview.token,key());await drainProvisioning(db);
 const data=await technologyData(db,actor);
 expect(data.applications.find(a=>a.id.startsWith(first.id))?.active).toBe(false);
 expect(data.applications.find(a=>a.id.startsWith(restored.id))?.active).toBe(true);
 expect((await db.pool.query('SELECT iam_assignment_active($1,$2) ok',[f.claimant.assignmentId,Date.now()])).rows[0].ok).toBe(false);
});
