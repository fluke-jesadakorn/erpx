import { afterEach,beforeAll, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import {
  expenseFixture,
  expenseDb as raw,
  key,
} from "../../support/helpers/expense-fixture";
import {installStaffLevels,savePosition,positionSchema} from '@erpx/server-organization/public/organization';
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
} from "@erpx/server-people_payroll/public/people-lifecycle";
import { drainPeopleLifecycle } from "@erpx/server-people_payroll/public/people-lifecycle-jobs";
import {
  packageInput,
  lifecycleInput,
  peopleIamDestination,
} from "@erpx/feature-people_payroll/lib/people-lifecycle";
import { iamDb } from "@erpx/server-identity_access/public/iam-db";
import {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import { iamActorFromWork } from "@erpx/server-identity_access/public/iam-context";
import { loadIamAuthority } from "@erpx/server-identity_access/public/iam-store";
import { selectWorkRole } from "@erpx/server-work_projection/public/work-role-store";
import { decideIamReview } from "@erpx/server-identity_access/public/iam-service";
import {
  managementCommand,
  readManagement,
} from "@erpx/server-identity_access/public/user-management";
import { setDirectoryStatus } from "@erpx/server-people_payroll/public/people-directory";
import { transitionEngagement } from "@erpx/server-people_payroll/public/people-payroll";
import {packageGrantStore,readCompanyPackageGrants} from "@erpx/server-commercial/public/package-grants";
import { companyMembers } from "@erpx/server-organization/public/member-directory";
import {changeAppointment} from '@erpx/server-organization/public/department-authority';
import { enrichFlowWork } from "@erpx/server-workflow/public/flow-work";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {
  attachmentBindingState,
  enableAttachmentV2ForTest,
  seedAttachmentSet,
} from "../../support/helpers/attachment-v2";
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: raw, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
vi.mock("@erpx/server-identity_access/public/iam-mfa", () => ({ requireFreshMfa: async () => {} }));
const db = iamDb(raw);
beforeAll(async () => {
  await applyMigrations(raw, env.STAFF_TEST_MIGRATIONS);
  await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS);
  enableAttachmentV2ForTest();
});
afterEach(()=>vi.unstubAllEnvs());
it('drains scheduled People work with the single connection used by development jobs',async()=>{
 const single=new PostgresDatabase({...db.pool.options,max:1,connectionTimeoutMillis:250});
 try{expect(await drainPeopleLifecycle(single,0)).toBe(0);}finally{await single.pool.end();}
});
async function purchaseCorePeople(companyId:string,state:'active'|'draining'='active',endsAt:number|null=null){const store=packageGrantStore(raw,{authorize:async(_tx,company)=>{if(company!==companyId)throw Error('ISOLATED_PEOPLE_PROVIDER_SCOPE');return {userId:'isolated-people-lifecycle-provider'};},obligations:async()=>{throw Error('No inactive cutover witness');}}),previous=(await readCompanyPackageGrants(raw,companyId)).find(row=>row.packageId==='people');return store.apply({expectedVersion:previous?.version??0,idempotencyKey:key(),grant:{companyId:companyId,packageId:'people',packageVersion:1,state,startsAt:0,endsAt,version:(previous?.version??0)+1,reason:'Isolated actual People lifecycle owner purchase',evidence:'test:people-lifecycle-purchase-owner'}});}
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
    hrMember = await f.member("hr", "hr_admin");
  await changeAppointment(raw, owner, {kind:"hr",departmentId:null,positionId:null,personId:`person-${hrMember.userId}`,expectedVersion:0,reason:"Appoint fixture HR reviewer"}, key());
  const hr = await authorize(hrMember);
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
it("keeps HR case details private while IT can track preparation and ordinary readers cannot open lifecycle data", async () => {
  const f = await fixture();
  const privateReason = `Private HR reason ${key()}`;
  const privateSnapshot = `Private review ${key()}`;
  const privateError = `Private failure ${key()}`;
  const privateClearance = `Private clearance ${key()}`;
  const input = lifecycleInput.parse({
    kind: "onboarding",
    expectedVersion: 0,
    personId: f.person,
    engagementId: f.engagement,
    positionId: f.position,
    managerPersonId: `person-${f.owner.userId}`,
    effectiveAt: Date.now() + 3600000,
    reason: privateReason,
  });
  const preview = await previewLifecycle(raw, f.hr, input);
  const saved = await saveLifecycle(raw, f.hr, input, key(), true, preview.token);
  await raw.prepare("UPDATE people_lifecycle_cases SET review_snapshot=?::jsonb,last_error=? WHERE id=?")
    .bind(JSON.stringify({ privateSnapshot }), privateError, saved.id).run();
  const financeTask = `private-clearance-${key()}`;
  await raw.prepare("INSERT INTO people_preparation_tasks(id,company_id,case_id,kind,title,status,updated_at,clearance_type,completion_reason) VALUES(?,?,?,'lifecycle',?,'pending',?,'finance',?)")
    .bind(financeTask, f.company, saved.id, privateClearance, Date.now(), privateClearance).run();

  const hrData = await peopleLifecycleData(raw, f.hr);
  const hrCase = hrData.cases.find(c => c.id === saved.id);
  expect(hrCase && "definition" in hrCase ? hrCase.definition.reason : undefined).toBe(privateReason);
  expect(JSON.stringify(hrCase)).toContain(privateSnapshot);
  expect(hrData.tasks.some(task => task.id === financeTask)).toBe(true);

  const itData = await peopleLifecycleData(raw, f.it);
  const itCase = itData.cases.find(c => c.id === saved.id);
  expect(itCase).toMatchObject({ id: saved.id, person_id: f.person, status: saved.status, last_error: "Preparation needs attention" });
  expect(itCase).not.toHaveProperty("definition");
  expect(itCase).not.toHaveProperty("review_snapshot");
  expect(itCase).not.toHaveProperty("created_by");
  expect(itData.tasks.some(task => task.id === financeTask)).toBe(false);
  const itWork = await peopleLifecycleWork(raw, f.it);
  expect(itWork.some(item => item.id === saved.id)).toBe(true);
  for (const privateValue of [privateReason, privateSnapshot, privateError, privateClearance]) {
    expect(JSON.stringify(itData)).not.toContain(privateValue);
    expect(JSON.stringify(itWork)).not.toContain(privateValue);
  }

  const readOnly = { ...f.it, iam: { ...f.it.iam!, permissions: ["people.read"] } };
  await expect(peopleLifecycleData(raw, readOnly)).rejects.toMatchObject({ status: 403 });
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
      f.it,
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
it("routes Company IAM aliases to the current People and Technology owners", () => {
  for (const section of ["users", "overview", "my-access"]) expect(peopleIamDestination(section)).toBe("technology?view=resources&section=accounts");
  expect(peopleIamDestination("connections")).toBe("technology?view=directory&section=connections");
  // Legacy IAM mappings are access-group mappings, distinct from Directory field mappings.
  expect(peopleIamDestination("mappings")).toBe("technology?view=settings&section=groups");
  for (const section of ["groups", "roles", "matrix", "reviews"]) expect(peopleIamDestination(section)).toBe(`technology?view=settings&section=${section}`);
  expect(peopleIamDestination("custom&section=roles")).toBe("technology?view=settings&section=custom%26section%3Droles");
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
      f.it,
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

it("allows IT to prepare groups and denies HR access-policy administration", async () => {
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
      iamActorFromWork(f.hr),
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
    f.it,
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
    (await companyMembers(f.hr)).find((m) => m.id === alias.userId),
  ).toBeUndefined();
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
 const {calendarCommand}=await import("@erpx/server-work_projection/public/calendar/commands");
 const {retirementWork}=await import("@erpx/server-people_payroll/public/retirement-clearance");
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

it("reads retirement evidence only for the assigned task owner and never changes the financial document",async()=>{
 const {calendarCommand}=await import("@erpx/server-work_projection/public/calendar/commands");
 const {clearanceCase}=await import("@erpx/server-people_payroll/public/retirement-clearance");
 const f=await fixture(),finance=await authorize(await f.member("clearance-evidence","finance_preparer")),otherFinance=await authorize(await f.member("clearance-evidence-other","finance_preparer")),expenseId=await f.claim(finance);
 const before=await raw.prepare("SELECT status,version FROM finance_expenses WHERE id=?").bind(expenseId).first();
 const input=lifecycleInput.parse({kind:"offboarding",departureReason:"retirement",expectedVersion:0,personId:f.person,engagementId:f.engagement,positionId:null,lastWorkingDate:"2026-12-31",effectiveAt:Date.now()+86400000,reason:"Retirement evidence review",clearanceOwners:{finance:finance.userId}});
 const saved=await saveLifecycle(raw,f.hr,input,key(),true,(await previewLifecycle(raw,f.hr,input)).token),task=(await peopleLifecycleData(raw,f.hr)).tasks.find(t=>t.case_id===saved.id&&t.clearance_type==="finance")!;
 await expect(calendarCommand(raw,finance,{action:"task.complete",id:task.id,expectedVersion:1,status:"completed",reason:"Review without source"},key())).rejects.toMatchObject({status:422});
 const files=await seedAttachmentSet(raw,finance,{targetType:"people_clearance_task",targetId:String(task.id),componentKind:"action",componentRef:"finance",purposeKey:"clearance_evidence",expectedTargetVersion:1});
 const command={action:"task.complete",id:task.id,expectedVersion:1,status:"completed",reason:"Reviewed source ledger; no payment performed",references:[{kind:"expense",id:expenseId}],attachmentSetIds:[files.setId]};
 const receipt=key();await calendarCommand(raw,finance,command,receipt);await calendarCommand(raw,finance,command,receipt);
 expect(await attachmentBindingState(raw,files.setId)).toMatchObject({set_state:"bound",link_state:"bound",binding_type:"retirement_clearance_completion",binding_id:String(task.id)});
 await expect(calendarCommand(raw,finance,command,key())).rejects.toMatchObject({status:409});
 expect(await raw.prepare("SELECT status,version FROM finance_expenses WHERE id=?").bind(expenseId).first()).toEqual(before);
 expect((await clearanceCase(raw,finance,String(saved.id))).tasks.find(t=>t.id===task.id)!.evidence[0].status).toBe("draft");
 const unrelatedFinanceTask=(await clearanceCase(raw,otherFinance,String(saved.id))).tasks.find(t=>t.id===task.id)!;
 expect(unrelatedFinanceTask.evidence).toEqual([]);
 expect(unrelatedFinanceTask.completion_reason).toBeNull();
 const hrTask=(await clearanceCase(raw,f.hr,String(saved.id))).tasks.find(t=>t.id===task.id)!;
 expect(hrTask.evidence).toEqual([]);
 expect(hrTask.completion_reason).toBeNull();
 expect(JSON.stringify(await peopleLifecycleData(raw,f.hr))).not.toContain(expenseId);
 const other=await fixture();await expect(clearanceCase(raw,other.hr,String(saved.id))).rejects.toMatchObject({status:404});
});
it("revalidates clearance owners, rejects stale edits and keeps reassignment notifications attached to the same task",async()=>{
 const {calendarCommand}=await import("@erpx/server-work_projection/public/calendar/commands");
 const {clearanceOwners}=await import("@erpx/server-people_payroll/public/retirement-clearance");
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
 const {peopleDirectoryPage}=await import("@erpx/server-people_payroll/public/people-directory-page");
 const f=await fixture();const prefix=key();
 await raw.batch(Array.from({length:65},(_,i)=>raw.prepare("INSERT INTO hr_people(id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES(?,?,?,'TH',?,1,1)").bind(`${prefix}-${i}`,f.company,`Pagination ${String(i).padStart(3,'0')}`,f.hr.userId)));
 await raw.batch(Array.from({length:65},(_,i)=>raw.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'employee','salaried','onboarding','2026-01-01','Draft employee','THB',?,1,1)").bind(`eng-${prefix}-${i}`,`${prefix}-${i}`,f.company,f.branch,f.department,f.hr.userId)));
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

it("submits and applies HR placement while Position GPO delivery is pending or blocked",async()=>{
  for(const directoryStatus of ["pending","blocked"] as const){
    const f=await fixture(),connection=`gpo-${key()}`,now=Date.now();
    const jobStatus=directoryStatus==="blocked"?"blocked":"queued";
    await raw.prepare("INSERT INTO iam_connections(id,company_id,name,provider,config,status,created_by,created_at) VALUES(?,?,'Position GPO','active_directory','{}'::jsonb,'active',?,?)").bind(connection,f.company,f.it.userId,now).run();
    await raw.prepare("INSERT INTO position_gpo_policies(company_id,position_id,version,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,directory_config_status,directory_config_error,updated_at) VALUES(?,?,1,1,?,'gpo-target','group-target','ou-target',?,?,?)")
      .bind(f.company,f.position,connection,directoryStatus,directoryStatus==="blocked"?"IT_PRIVATE_CONFIG_ERROR":null,now).run();
    const engagementVersion=await raw.prepare("SELECT version FROM hr_engagements WHERE id=?").bind(f.engagement).first<number>("version");
    await raw.prepare(`INSERT INTO position_gpo_jobs(id,company_id,position_id,person_id,revision,engagement_version,connection_id,gpo_id,filter_group_id,computer_target_ou_id,desired_action,status,last_error,created_at,updated_at,engagement_id,account_id)
      VALUES(?,?,?,?,1,?,?,'gpo-target','group-target','ou-target','add',?,?,?,?,?,'account-target')`)
      .bind(`gpo-job-${key()}`,f.company,f.position,f.person,engagementVersion,connection,jobStatus,jobStatus==="blocked"?"IT_PRIVATE_JOB_ERROR":null,now,now,f.engagement).run();

    const effectiveAt=Date.now()+60000;
    const input=lifecycleInput.parse({expectedVersion:0,kind:"transfer",personId:f.person,engagementId:f.engagement,positionId:f.position,managerPersonId:`person-${f.owner.userId}`,effectiveAt,reason:`HR placement with ${directoryStatus} GPO`});
    const preview=await previewLifecycle(raw,f.hr,input,true);
    expect(preview.ready,`${directoryStatus} GPO leaves HR placement ready`).toBe(true);
    expect(preview.blockers).toEqual([]);
    const placed=await saveLifecycle(raw,f.hr,input,key(),true,preview.token,true);
    expect(placed.status).toBe("scheduled");
    await drainPeopleLifecycle(raw,effectiveAt+1);
    expect(await raw.prepare("SELECT status,last_error FROM people_lifecycle_cases WHERE id=?").bind(placed.id).first()).toMatchObject({status:"completed",last_error:null});
    expect(await raw.prepare("SELECT position_id,status FROM hr_engagements WHERE id=?").bind(f.engagement).first()).toMatchObject({position_id:f.position,status:"active"});
    const data=await peopleLifecycleData(raw,f.hr);
    expect(data.positions.find(p=>p.id===f.position)?.gpo).toEqual({mapped:true,directoryStatus});
    expect(data.roster.find(p=>p.engagementId===f.engagement)?.gpoDelivery).toBe(directoryStatus==="blocked"?"blocked":"awaiting_identity");
  }
});

async function scheduledHrPlacement(f:Awaited<ReturnType<typeof fixture>>,effectiveAt=Date.now()+60000){
  const input=lifecycleInput.parse({expectedVersion:0,kind:"transfer",personId:f.person,engagementId:f.engagement,positionId:f.position,managerPersonId:`person-${f.owner.userId}`,effectiveAt,reason:"Scheduled HR placement"});
  const preview=await previewLifecycle(raw,f.hr,input,true);
  const result=await saveLifecycle(raw,f.hr,input,key(),true,preview.token,true);
  return {id:String(result.id),effectiveAt};
}

it("applies scheduled HR placement after an unrelated engagement edit",async()=>{
  const f=await fixture(),scheduled=await scheduledHrPlacement(f);
  await raw.prepare("UPDATE hr_engagements SET currency='THB',version=version+1 WHERE id=?").bind(f.engagement).run();
  await drainPeopleLifecycle(raw,scheduled.effectiveAt+1);
  expect(await raw.prepare("SELECT status,last_error FROM people_lifecycle_cases WHERE id=?").bind(scheduled.id).first()).toMatchObject({status:"completed",last_error:null});
  expect(await raw.prepare("SELECT position_id,currency FROM hr_engagements WHERE id=?").bind(f.engagement).first()).toMatchObject({position_id:f.position,currency:"THB"});
});

it("holds scheduled HR placement after a placement change and fails closed on old snapshots",async()=>{
  const changed=await fixture(),newCase=await scheduledHrPlacement(changed);
  await raw.prepare("UPDATE hr_engagements SET position_id=?,version=version+1 WHERE id=?").bind(changed.ceo,changed.engagement).run();
  await drainPeopleLifecycle(raw,newCase.effectiveAt+1);
  expect(await raw.prepare("SELECT status,last_error FROM people_lifecycle_cases WHERE id=?").bind(newCase.id).first()).toMatchObject({status:"scheduled",last_error:"LIFECYCLE_PLACEMENT_CHANGED"});
  expect(await raw.prepare("SELECT position_id FROM hr_engagements WHERE id=?").bind(changed.engagement).first("position_id")).toBe(changed.ceo);

  const older=await fixture(),oldCase=await scheduledHrPlacement(older);
  await raw.prepare("UPDATE people_lifecycle_cases SET review_snapshot=review_snapshot-'engagementPlacement' WHERE id=?").bind(oldCase.id).run();
  await raw.prepare("UPDATE hr_engagements SET currency='THB',version=version+1 WHERE id=?").bind(older.engagement).run();
  await drainPeopleLifecycle(raw,oldCase.effectiveAt+1);
  expect(await raw.prepare("SELECT status,last_error FROM people_lifecycle_cases WHERE id=?").bind(oldCase.id).first()).toMatchObject({status:"scheduled",last_error:"LIFECYCLE_PLACEMENT_CHANGED"});
});

it("requires explicit confirmation to supersede a legacy waiting case while keeping its history",async()=>{
 const f=await fixture();
 const legacyInput=lifecycleInput.parse({expectedVersion:0,kind:"transfer",personId:f.person,engagementId:f.engagement,positionId:f.position,managerPersonId:`person-${f.owner.userId}`,effectiveAt:Date.now()+60000,reason:"Old access-led transfer"});
 const oldPreview=await previewLifecycle(raw,f.hr,legacyInput);
 const old=await saveLifecycle(raw,f.hr,legacyInput,key(),true,oldPreview.token);
 expect(old.status).toBe("waiting_it");
 const placementInput=lifecycleInput.parse({...legacyInput,reason:"HR placement without access approval"});
 const preview=await previewLifecycle(raw,f.hr,placementInput,true);
 expect(preview.ready).toBe(true);
 expect(preview.legacyCaseToSupersede).toMatchObject({id:old.id,version:old.version,status:"waiting_it"});
 await expect(saveLifecycle(raw,f.hr,placementInput,key(),true,preview.token,true)).rejects.toMatchObject({code:"LEGACY_SUPERSEDE_REQUIRED"});
 await expect(saveLifecycle(raw,f.hr,placementInput,key(),true,preview.token,true,{id:String(old.id),expectedVersion:old.version+1})).rejects.toMatchObject({code:"LEGACY_CASE_CHANGED"});
 const command=key(),result=await saveLifecycle(raw,f.hr,placementInput,command,true,preview.token,true,{id:String(old.id),expectedVersion:old.version});
 expect(result.status).toBe("scheduled");
 expect(await saveLifecycle(raw,f.hr,placementInput,command,true,preview.token,true,{id:String(old.id),expectedVersion:old.version})).toEqual(result);
 expect(await raw.prepare("SELECT status,version,workflow FROM people_lifecycle_cases WHERE id=?").bind(old.id).first()).toMatchObject({status:"cancelled",version:old.version+1,workflow:"legacy"});
 expect(await raw.prepare("SELECT status,workflow,package_id FROM people_lifecycle_cases WHERE id=?").bind(result.id).first()).toMatchObject({status:"scheduled",workflow:"placement",package_id:null});
 expect(await raw.prepare("SELECT count(*)::integer n FROM audit_events WHERE command_id=? AND action='people.lifecycle.supersede' AND record_id=?").bind(command,old.id).first("n")).toBe(1);
});

it("superseding a reviewed legacy case closes its pending reviews and preparation tasks",async()=>{
 const f=await fixture();await prepare(f);
 const input=lifecycleInput.parse({expectedVersion:0,kind:"transfer",personId:f.person,engagementId:f.engagement,positionId:f.position,managerPersonId:`person-${f.owner.userId}`,effectiveAt:Date.now()+60000,reason:"Old reviewed transfer"});
 const oldPreview=await previewLifecycle(raw,f.hr,input),old=await saveLifecycle(raw,f.hr,input,key(),true,oldPreview.token);
 expect(old.status).toBe("pending_review");
 const requested=await requestLifecycleAccess(raw,f.it,String(old.id),old.version,key());
 expect(requested.reviews.length).toBeGreaterThan(0);
 const replacement=lifecycleInput.parse({...input,reason:"HR placement after old review"}),preview=await previewLifecycle(raw,f.hr,replacement,true);
 expect(preview.legacyCaseToSupersede).toMatchObject({id:old.id,status:"pending_review"});
 await saveLifecycle(raw,f.hr,replacement,key(),true,preview.token,true,{id:String(old.id),expectedVersion:old.version+1});
 expect(await raw.prepare("SELECT status FROM iam_reviews WHERE id=?").bind(requested.reviews[0]).first("status")).toBe("failed");
  expect(await raw.prepare("SELECT count(*)::integer n FROM people_preparation_tasks WHERE case_id=? AND status='pending'").bind(old.id).first("n")).toBe(0);
});

it("superseding a scheduled legacy case revokes its approved grant before HR placement",async()=>{
 const f=await fixture();await prepare(f);
 const input=lifecycleInput.parse({expectedVersion:0,kind:"transfer",personId:f.person,engagementId:f.engagement,positionId:f.position,managerPersonId:`person-${f.owner.userId}`,effectiveAt:Date.now()+60000,reason:"Old scheduled access"});
 const oldPreview=await previewLifecycle(raw,f.hr,input),old=await saveLifecycle(raw,f.hr,input,key(),true,oldPreview.token);
 const requested=await requestLifecycleAccess(raw,f.it,String(old.id),old.version,key()),reviewer=iamActorFromWork(f.owner);
 for(const reviewId of requested.reviews)await decideIamReview(raw,reviewer,reviewId,{expectedVersion:1,decision:"approve",reason:"Approve old access"},key(),(c,kind,p)=>applyLifecycleReview(c,reviewer,kind,p));
 const scheduled=await raw.prepare("SELECT version,status FROM people_lifecycle_cases WHERE id=?").bind(old.id).first<{version:number;status:string}>();
 expect(scheduled?.status).toBe("scheduled");
 const grantId=await raw.prepare("SELECT assignment_id FROM people_lifecycle_grants WHERE case_id=? LIMIT 1").bind(old.id).first<string>("assignment_id");
 expect(grantId).toBeTruthy();
 const replacement=lifecycleInput.parse({...input,reason:"HR-only replacement"}),preview=await previewLifecycle(raw,f.hr,replacement,true);
 await saveLifecycle(raw,f.hr,replacement,key(),true,preview.token,true,{id:String(old.id),expectedVersion:scheduled!.version});
 expect(await raw.prepare("SELECT revoked_at IS NOT NULL AS revoked FROM assignments WHERE id=?").bind(grantId).first("revoked")).toBe(true);
 expect(await raw.prepare("SELECT status FROM iam_reviews WHERE id=?").bind(requested.reviews[0]).first("status")).toBe("approved");
});

it("shows Position GPO mapping and employee Directory delivery without exposing targets",async()=>{
 const f=await fixture(),connection=`gpo-${key()}`,identity=`identity-${key()}`,now=Date.now();
 await raw.prepare("UPDATE hr_engagements SET position_id=?,status='active',version=version+1 WHERE id=?").bind(f.position,f.engagement).run();
 expect((await peopleLifecycleData(raw,f.hr)).roster.find(r=>r.engagementId===f.engagement)?.gpoDelivery).toBe("none");
 await raw.prepare("INSERT INTO iam_connections(id,company_id,name,provider,config,status,created_by,created_at) VALUES(?,?,'Position GPO','active_directory','{}'::jsonb,'active',?,?)").bind(connection,f.company,f.it.userId,now).run();
 await raw.prepare("INSERT INTO position_gpo_policies(company_id,position_id,version,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,updated_at) VALUES(?,?,1,1,?,'gpo-target','group-target','ou-target',?)").bind(f.company,f.position,connection,now).run();
 let data=await peopleLifecycleData(raw,f.hr);
 expect(data.positions.find(p=>p.id===f.position)?.gpo).toEqual({mapped:true,directoryStatus:"pending"});
 expect(data.positions.find(p=>p.id===f.ceo)?.gpo).toEqual({mapped:false,directoryStatus:null});
 expect(data.roster.find(r=>r.engagementId===f.engagement)?.gpoDelivery).toBe("awaiting_identity");
 await raw.prepare("INSERT INTO iam_identities(id,company_id,connection_id,external_id,username,name,enabled,observed_at,user_id,person_id) VALUES(?,?,?,'account-target','employee','Employee',true,?,?,?)").bind(identity,f.company,connection,now,f.claimant.userId,f.person).run();
 await raw.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,connection_id,identity_id) VALUES(?,?,?,?,'active','directory',?,?) ON CONFLICT(company_id,user_id) DO UPDATE SET person_id=excluded.person_id,status='active',login_policy='directory',connection_id=excluded.connection_id,identity_id=excluded.identity_id").bind(f.company,f.claimant.userId,`directory:${identity}`,f.person,connection,identity).run();
 const engagementVersion=await raw.prepare("SELECT version FROM hr_engagements WHERE id=?").bind(f.engagement).first<number>("version");
 const job=`gpo-job-${key()}`;
 await raw.prepare(`INSERT INTO position_gpo_jobs(id,company_id,position_id,person_id,revision,engagement_version,connection_id,gpo_id,filter_group_id,computer_target_ou_id,desired_action,status,created_at,updated_at,engagement_id,account_id)
   VALUES(?,?,?,?,1,?,?, 'gpo-target','group-target','ou-target','add','queued',?,?,?, 'account-target')`)
   .bind(job,f.company,f.position,f.person,engagementVersion,connection,now,now,f.engagement).run();
 data=await peopleLifecycleData(raw,f.hr);
 expect(data.roster.find(r=>r.engagementId===f.engagement)?.gpoDelivery).toBe("queued");
 await raw.prepare("UPDATE position_gpo_jobs SET status='directory_applied' WHERE id=?").bind(job).run();
 await raw.prepare("UPDATE position_gpo_policies SET directory_config_status='directory_applied' WHERE company_id=? AND position_id=?").bind(f.company,f.position).run();
 data=await peopleLifecycleData(raw,f.hr);
 expect(data.roster.find(r=>r.engagementId===f.engagement)?.gpoDelivery).toBe("directory_applied");
 expect(data.positions.find(p=>p.id===f.position)?.gpo).toEqual({mapped:true,directoryStatus:"directory_applied"});
 expect(JSON.stringify(data.positions.find(p=>p.id===f.position)?.gpo)).not.toContain(connection);
 await raw.prepare("UPDATE position_gpo_jobs SET status='blocked',last_error='PRIVATE_PROVIDER_DETAIL' WHERE id=?").bind(job).run();
 data=await peopleLifecycleData(raw,{...f.hr,branchId:f.branch,departmentId:f.department});
 expect(data.positions.every(p=>p.id!==f.ceo)).toBe(true);
 expect(data.roster.find(r=>r.engagementId===f.engagement)?.gpoDelivery).toBe("blocked");
 expect(JSON.stringify(data.roster.find(r=>r.engagementId===f.engagement)?.gpoDelivery)).not.toContain("PRIVATE_PROVIDER_DETAIL");
 await raw.prepare("UPDATE position_gpo_policies SET connection_id=NULL,gpo_id=NULL,user_filter_group_id=NULL,computer_target_ou_id=NULL,version=version+1,revision=revision+1 WHERE company_id=? AND position_id=?").bind(f.company,f.position).run();
 data=await peopleLifecycleData(raw,f.hr);
 expect(data.positions.find(p=>p.id===f.position)?.gpo).toEqual({mapped:false,directoryStatus:null});
 expect(data.roster.find(r=>r.engagementId===f.engagement)?.gpoDelivery).toBe("none");
 const inherited=`position-rule-${key()}`;
 await raw.prepare(`INSERT INTO organization_gpo_rules(id,company_id,target_kind,target_id,version,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,directory_config_status,last_actor_id,last_reason,updated_at)
   VALUES(?,?,'position',?,1,1,?,'inherited-gpo','inherited-group','ou-target','directory_applied',?,'Inherited delivery test',?)`)
   .bind(inherited,f.company,f.position,connection,f.it.userId,now).run();
 data=await peopleLifecycleData(raw,f.hr);
 expect(data.positions.find(p=>p.id===f.position)?.gpo).toEqual({mapped:true,directoryStatus:"directory_applied"});
 // The removed legacy rule's blocked job must not obscure a new direct mapping.
 expect(data.roster.find(r=>r.engagementId===f.engagement)?.gpoDelivery).toBe("queued");
 await raw.prepare(`INSERT INTO position_gpo_jobs(id,company_id,position_id,person_id,revision,engagement_version,connection_id,gpo_id,filter_group_id,computer_target_ou_id,desired_action,status,created_at,updated_at,engagement_id,account_id,rule_id,source_id)
   VALUES(?,?,?,?,1,?,?,'inherited-gpo','inherited-group','ou-target','add','directory_applied',?,?,?,'account-target',?,?)`)
   .bind(`inherited-job-${key()}`,f.company,f.position,f.person,engagementVersion,connection,now,now,f.engagement,inherited,`organization-gpo:${inherited}:${f.engagement}`).run();
 expect((await peopleLifecycleData(raw,f.hr)).roster.find(r=>r.engagementId===f.engagement)?.gpoDelivery).toBe("directory_applied");
 const secondConnection=`unlinked-${key()}`;
 await raw.prepare("INSERT INTO iam_connections(id,company_id,name,provider,config,status,created_by,created_at) VALUES(?,?,'Second directory','active_directory','{}'::jsonb,'active',?,?)").bind(secondConnection,f.company,f.it.userId,now).run();
 await raw.prepare(`INSERT INTO organization_gpo_rules(id,company_id,target_kind,target_id,version,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,directory_config_status,last_actor_id,last_reason,updated_at)
   VALUES(?,?,'position',?,1,1,?,'second-gpo','second-group','second-ou','directory_applied',?,'Independent directory test',?)`)
   .bind(`second-position-rule-${key()}`,f.company,f.position,secondConnection,f.it.userId,now).run();
 expect((await peopleLifecycleData(raw,f.hr)).roster.find(r=>r.engagementId===f.engagement)?.gpoDelivery).toBe("awaiting_identity");
});

it("IT applies pinned resources and requests independently reviewed ERPX roles, retaining them across HR transfers",async()=>{
  const {previewProvisioning,applyProvisioning,drainProvisioning,applyProvisioningReview,technologyData}=await import("@erpx/server-identity_access/public/technology");
  const {provisioningInput}=await import("@erpx/server-identity_access/public/rules/technology");
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
 const {previewProvisioning,applyProvisioning,drainProvisioning,technologyData}=await import("@erpx/server-identity_access/public/technology");
 const {provisioningInput}=await import("@erpx/server-identity_access/public/rules/technology");
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
 const {previewProvisioning,applyProvisioning,drainProvisioning,technologyData}=await import("@erpx/server-identity_access/public/technology");
 const {provisioningInput}=await import("@erpx/server-identity_access/public/rules/technology");
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
 const {previewProvisioning,applyProvisioning,drainProvisioning,technologyData}=await import("@erpx/server-identity_access/public/technology");
 const {provisioningInput}=await import("@erpx/server-identity_access/public/rules/technology");
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

it("rolls back position binding, preparation, outbox and IAM receipt after a late audit failure, then retries and replays once", async () => {
  const f = await fixture(), input = packageInput.parse({ expectedVersion: 0, reason: "Reviewed shared access package", definition: { name: "Atomic position package", account: "local", groupIds: [], grants: [], tasks: [] } });
  const preview = await previewPackage(raw, f.it, input), pkg = await savePackage(raw, f.it, input, key(), true, preview.token);
  const binding = { positionId: f.position, packageId: pkg.id, revision: pkg.revision, expectedVersion: 1 };
  const snapshot = await bindPositionPackage(raw, f.it, binding, key(), true), command = key(), payload = { ...binding, previewToken: snapshot.token };
  const fault = `people_position_fault_${command.replaceAll('-', '_')}`;
  await raw.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'PEOPLE_POSITION_FINAL_AUDIT_FAILURE' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} BEFORE INSERT ON iam_audit FOR EACH ROW WHEN(NEW.target_id='${command}') EXECUTE FUNCTION ${fault}();`);
  try {
    await expect(bindPositionPackage(raw, f.it, payload, command)).rejects.toThrow('PEOPLE_POSITION_FINAL_AUDIT_FAILURE');
    expect(await raw.prepare('SELECT version FROM organization_positions WHERE id=?').bind(f.position).first('version')).toBe(1);
    expect(await raw.prepare('SELECT count(*) n FROM people_position_packages WHERE company_id=? AND position_id=?').bind(f.company, f.position).first('n')).toBe(0);
    expect(await raw.prepare("SELECT status FROM people_preparation_tasks WHERE position_id=? AND kind='position'").bind(f.position).first('status')).toBe('pending');
    expect(await raw.prepare('SELECT count(*) n FROM outbox_events WHERE command_id=?').bind(command).first('n')).toBe(0);
    expect(await raw.prepare('SELECT count(*) n FROM iam_receipts WHERE id=?').bind(command).first('n')).toBe(0);
    expect(await raw.prepare('SELECT count(*) n FROM iam_audit WHERE target_id=?').bind(command).first('n')).toBe(0);
  } finally { await raw.exec(`DROP TRIGGER ${fault} ON iam_audit;DROP FUNCTION ${fault}();`); }
  const result = await bindPositionPackage(raw, f.it, payload, command);
  expect(await bindPositionPackage(raw, f.it, payload, command)).toEqual(result);
  expect(await raw.prepare('SELECT version FROM organization_positions WHERE id=?').bind(f.position).first('version')).toBe(2);
  expect(await raw.prepare('SELECT count(*) n FROM people_position_packages WHERE company_id=? AND position_id=?').bind(f.company, f.position).first('n')).toBe(1);
  expect(await raw.prepare("SELECT status FROM people_preparation_tasks WHERE position_id=? AND kind='position'").bind(f.position).first('status')).toBe('completed');
  for (const sql of ['SELECT count(*) n FROM outbox_events WHERE command_id=?', 'SELECT count(*) n FROM iam_receipts WHERE id=?', 'SELECT count(*) n FROM iam_audit WHERE target_id=?']) expect(await raw.prepare(sql).bind(command).first('n')).toBe(1);
});

it("preserves atomic provisioning apply and worker rollback followed by the separate durable retry journal",async()=>{
 const {previewProvisioning,applyProvisioning,drainProvisioning,retryProvisioning}=await import("@erpx/server-identity_access/public/technology");const {provisioningInput}=await import("@erpx/server-identity_access/public/rules/technology");const f=await fixture();await prepare(f);await db.pool.query('UPDATE hr_engagements SET position_id=$2 WHERE id=$1',[f.engagement,f.position]);const actor=iamActorFromWork(f.it),input=provisioningInput.parse({engagementIds:[f.engagement],validUntil:Date.now()+3600000,exclude:['role:employee'],reason:'Atomic Technology journal proof'}),preview=await previewProvisioning(db,actor,input),command=key(),fault=`technology_apply_fault_${command.replaceAll('-','_')}`;
 const tables=['people_provision_operations','people_applied_packages','people_provision_grants','people_preparation_tasks','people_directory_operations','people_directory_membership_sources','assignments','iam_reviews','iam_receipts','iam_audit','outbox_events'];async function state(table:string){return (await db.pool.query(`SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb) state FROM ${table} t`)).rows[0].state;}
 const before=await Promise.all(tables.map(state));await raw.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'TECHNOLOGY_FINAL_APPLY_AUDIT_FAILURE' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON iam_audit FOR EACH ROW WHEN(NEW.target_id='${command}') EXECUTE FUNCTION ${fault}();`);
 try{await expect(applyProvisioning(db,actor,input,preview.token,command)).rejects.toThrow('TECHNOLOGY_FINAL_APPLY_AUDIT_FAILURE');expect(await Promise.all(tables.map(state))).toEqual(before);}finally{await raw.exec(`DROP TRIGGER ${fault} ON iam_audit;DROP FUNCTION ${fault}();`);}
 const applied=await applyProvisioning(db,actor,input,preview.token,command);expect(await applyProvisioning(db,actor,input,preview.token,command)).toEqual(applied);expect((await db.pool.query('SELECT count(*)::int n FROM iam_receipts WHERE id=$1',[command])).rows[0].n).toBe(1);
 const workerTables=tables.filter(t=>t!=='people_provision_operations'),workerBefore=await Promise.all(workerTables.map(state)),workerFault=`technology_worker_fault_${command.replaceAll('-','_')}`;
 await raw.exec(`CREATE FUNCTION ${workerFault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'TECHNOLOGY_FINAL_WORKER_OUTBOX_FAILURE' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${workerFault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN(NEW.command_id='provision:${applied.id}') EXECUTE FUNCTION ${workerFault}();`);
 try{await drainProvisioning(db);expect(await Promise.all(workerTables.map(state))).toEqual(workerBefore);expect((await db.pool.query('SELECT status,last_error,version FROM people_provision_operations WHERE id=$1',[applied.id])).rows[0]).toEqual({status:'blocked',last_error:'PROVISIONING_RETRY_REQUIRED',version:2});}finally{await raw.exec(`DROP TRIGGER ${workerFault} ON outbox_events;DROP FUNCTION ${workerFault}();`);}
 const retryKey=key(),retried=await retryProvisioning(db,actor,applied.id,2,retryKey);expect(await retryProvisioning(db,actor,applied.id,2,retryKey)).toEqual(retried);await drainProvisioning(db);await drainProvisioning(db);expect((await db.pool.query('SELECT status,last_error,version FROM people_provision_operations WHERE id=$1',[applied.id])).rows[0]).toEqual({status:'completed',last_error:null,version:4});expect((await db.pool.query('SELECT count(*)::int n FROM outbox_events WHERE command_id=$1',['provision:'+applied.id])).rows[0].n).toBe(1);expect((await db.pool.query("SELECT count(*)::int n FROM iam_audit WHERE action='people.provision.applied' AND target_id=$1",[applied.id])).rows[0].n).toBe(1);
});

it('People core lifecycle drains retained departures without letting an older unpurchased placement consume the worker limit',async()=>{
 const f=await fixture(),g=await fixture(),effectiveAt=Date.now()+60_000,input=lifecycleInput.parse({expectedVersion:0,kind:'transfer',personId:f.person,engagementId:f.engagement,positionId:f.position,managerPersonId:`person-${f.owner.userId}`,effectiveAt,reason:'Active People placement becomes draining before application'});
 vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');await expect(previewLifecycle(raw,f.hr,input,true)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await purchaseCorePeople(f.company);const preview=await previewLifecycle(raw,f.hr,input,true),command=key(),created=await saveLifecycle(raw,f.hr,input,command,true,preview.token,true);expect(created.status).toBe('scheduled');await purchaseCorePeople(f.company,'draining');
 await expect(saveLifecycle(raw,f.hr,{...input,id:String(created.id),expectedVersion:1,reason:'Amending an incoming placement while draining'},key(),false,undefined,true)).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 await purchaseCorePeople(g.company);await purchaseCorePeople(g.company,'draining');const outgoing=lifecycleInput.parse({expectedVersion:0,kind:'offboarding',personId:g.person,engagementId:g.engagement,positionId:null,effectiveAt:effectiveAt+10,lastWorkingDate:'2026-09-16',reason:'Existing reviewed employee departure while package drains'}),departurePreview=await previewLifecycle(raw,g.hr,outgoing),departure=await saveLifecycle(raw,g.hr,outgoing,key(),true,departurePreview.token);
 expect(await drainPeopleLifecycle(raw,effectiveAt+20)).toBe(1);expect(await raw.prepare('SELECT status FROM people_lifecycle_cases WHERE id=?').bind(created.id).first('status')).toBe('scheduled');expect(await raw.prepare('SELECT status FROM hr_engagements WHERE id=?').bind(g.engagement).first('status')).toBe('ended');expect(await raw.prepare('SELECT status FROM people_lifecycle_cases WHERE id=?').bind(departure.id).first('status')).toBe('completed');expect(await raw.prepare("SELECT COUNT(*) n FROM outbox_events WHERE aggregate_id=? AND event_type='people.lifecycle.applied'").bind(created.id).first('n')).toBe(0);
 expect(await cancelLifecycle(raw,f.hr,String(created.id),1,key())).toMatchObject({id:created.id,status:'cancelled'});expect(await saveLifecycle(raw,f.hr,input,command,true,preview.token,true)).toEqual(created);
});
it.each(['source','source-payload','authority','expiry'] as const)('People core lifecycle rolls deferred %s changes back before applying actual queued placement',async(change)=>{
 const f=await fixture(),effectiveAt=Date.now()+1000,input=lifecycleInput.parse({expectedVersion:0,kind:'transfer',personId:f.person,engagementId:f.engagement,positionId:f.position,managerPersonId:`person-${f.owner.userId}`,effectiveAt,reason:'Actual queued placement with final source and authority fence'});await purchaseCorePeople(f.company);vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const preview=await previewLifecycle(raw,f.hr,input,true),created=await saveLifecycle(raw,f.hr,input,key(),true,preview.token,true),before=await raw.prepare('SELECT position_id,status,version FROM hr_engagements WHERE id=?').bind(f.engagement).first(),authority=await raw.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(f.hr.assignmentId).first(),fn=`people_job_${key().replaceAll('-','')}`;
 if(change==='expiry')await purchaseCorePeople(f.company,'active',Date.now()+1300);
 const effect=change==='source'?`UPDATE people_lifecycle_cases SET kind='offboarding' WHERE id='${created.id}'`:change==='source-payload'?`UPDATE people_lifecycle_cases SET definition=jsonb_set(definition,'{reason}','\"Changed after queued effects\"'::jsonb),authorizing_assignment_id='${f.owner.assignmentId}' WHERE id='${created.id}'`:change==='authority'?`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.hr.assignmentId}'`:'PERFORM pg_sleep(1.4)';await raw.exec(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.aggregate_id='${created.id}' AND NEW.event_type='people.lifecycle.applied') EXECUTE FUNCTION ${fn}()`);
 // Other fictional Companies can have eligible retry jobs from earlier cases.
 // The assertions below fence this exact source, authority and outbox.
 try{await drainPeopleLifecycle(raw,effectiveAt+20);}finally{await raw.exec(`DROP TRIGGER ${fn} ON outbox_events;DROP FUNCTION ${fn}()`);}
 expect(await raw.prepare('SELECT position_id,status,version FROM hr_engagements WHERE id=?').bind(f.engagement).first()).toEqual(before);expect(await raw.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(f.hr.assignmentId).first()).toEqual(authority);expect(await raw.prepare('SELECT kind,status,version,attempts,last_error FROM people_lifecycle_cases WHERE id=?').bind(created.id).first()).toMatchObject({kind:'transfer',status:'scheduled',version:1,attempts:1,last_error:change.startsWith('source')?'LIFECYCLE_PURCHASE_SOURCE_CHANGED':change==='authority'?'LIFECYCLE_AUTHORITY_CHANGED':'PACKAGE_CAPABILITY_CHANGED'});expect(await raw.prepare("SELECT COUNT(*) n FROM outbox_events WHERE aggregate_id=? AND event_type='people.lifecycle.applied'").bind(created.id).first('n')).toBe(0);
});

async function peopleIamCase(f:Awaited<ReturnType<typeof fixture>>){
 await prepare(f);const input=lifecycleInput.parse({kind:'transfer',expectedVersion:0,personId:f.person,engagementId:f.engagement,positionId:f.position,managerPersonId:`person-${f.owner.userId}`,effectiveAt:Date.now()+60000,reason:'Actual package-reviewed lifecycle owner source'}),preview=await previewLifecycle(raw,f.hr,input);return saveLifecycle(raw,f.hr,input,key(),true,preview.token);
}
it('People IAM preserves Projects foundation recipes and position binding without buying full People',async()=>{
 const f=await fixture();vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const input=packageInput.parse({expectedVersion:0,reason:'Shared identity foundation recipe',definition:{name:'Projects worker access preparation',account:'local',groupIds:[],grants:[],tasks:[]}});
 await expect(previewPackage(raw,f.it,input)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 const store=packageGrantStore(raw,{authorize:async()=>({userId:'isolated-foundation-provider'}),obligations:async()=>{throw Error('No inactive witness');}});await store.apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:f.company,packageId:'projects',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Projects included identity foundation',evidence:'test:people-iam-foundation'}});
 const preview=await previewPackage(raw,f.it,input),command=key(),pkg=await savePackage(raw,f.it,input,command,true,preview.token);expect(await savePackage(raw,f.it,input,command,true,preview.token)).toEqual(pkg);
 const binding={positionId:f.position,packageId:pkg.id,revision:pkg.revision,expectedVersion:1},p=await bindPositionPackage(raw,f.it,binding,key(),true);await bindPositionPackage(raw,f.it,{...binding,previewToken:p.token},key());
 await expect(previewLifecycle(raw,f.hr,lifecycleInput.parse({kind:'transfer',expectedVersion:0,personId:f.person,engagementId:f.engagement,positionId:f.position,effectiveAt:Date.now()+60000,reason:'Projects does not grant private People lifecycle'}))).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
});
it('People IAM gates actual request and independent review, retaining replay and rejection while draining',async()=>{
 const f=await fixture(),created=await peopleIamCase(f);vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');await expect(requestLifecycleAccess(raw,f.it,String(created.id),1,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await purchaseCorePeople(f.company);const requestKey=key(),requested=await requestLifecycleAccess(raw,f.it,String(created.id),1,requestKey);await purchaseCorePeople(f.company,'draining');expect(await requestLifecycleAccess(raw,f.it,String(created.id),1,requestKey)).toEqual(requested);
 const reviewer=iamActorFromWork(f.owner),reviewId=requested.reviews[0];await expect(decideIamReview(raw,reviewer,reviewId,{decision:'approve',expectedVersion:1,reason:'Incoming authority needs active People'},key(),(c,k,p)=>applyLifecycleReview(c,reviewer,k,p))).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 const command=key(),payload={decision:'reject' as const,expectedVersion:1,reason:'Close retained incoming request during drain'},result=await decideIamReview(raw,reviewer,reviewId,payload,command,undefined,(c,k,p)=>applyLifecycleReview(c,reviewer,k,p,'reject'));expect(result.status).toBe('rejected');expect(await decideIamReview(raw,reviewer,reviewId,payload,command)).toEqual(result);expect(await raw.prepare('SELECT status FROM people_lifecycle_cases WHERE id=?').bind(created.id).first('status')).toBe('blocked');
});
it.each(['source','authority','proposer','expiry'] as const)('People IAM rolls late review %s changes back with assignments, receipts, decisions and audit',async(change)=>{
 const f=await fixture(),created=await peopleIamCase(f),requested=await requestLifecycleAccess(raw,f.it,String(created.id),1,key()),reviewer=iamActorFromWork(f.owner),reviewId=requested.reviews[0],command=key(),payload={decision:'approve' as const,expectedVersion:1,reason:'Verify whole original IAM final unit'},fn=`people_iam_${key().replaceAll('-','')}`;await purchaseCorePeople(f.company,'active',change==='expiry'?Date.now()+1300:null);vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const tables=['people_lifecycle_cases','people_lifecycle_grants','assignments','iam_reviews','iam_receipts','iam_audit','outbox_events'];const state=async(table:string)=>(await db.pool.query(`SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb) state FROM ${table} t`)).rows[0].state,before=await Promise.all(tables.map(state));
 const effect=change==='source'?`UPDATE people_lifecycle_cases SET definition=jsonb_set(definition,'{reason}','"Deferred source replacement"'::jsonb) WHERE id='${created.id}'`:change==='authority'||change==='proposer'?`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${change==='authority'?f.owner.assignmentId:f.it.assignmentId}'`:'PERFORM pg_sleep(1.4)';await raw.exec(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON iam_audit DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.target_id='${command}') EXECUTE FUNCTION ${fn}()`);
 try{await expect(decideIamReview(raw,reviewer,reviewId,payload,command,(c,k,p)=>applyLifecycleReview(c,reviewer,k,p))).rejects.toMatchObject({code:change==='source'?'PEOPLE_IAM_SOURCE_CHANGED':change==='authority'?'PEOPLE_IAM_AUTHORITY_CHANGED':change==='proposer'?'PEOPLE_IAM_PROPOSER_CHANGED':'PACKAGE_CAPABILITY_CHANGED'});expect(await Promise.all(tables.map(state))).toEqual(before);}finally{await raw.exec(`DROP TRIGGER ${fn} ON iam_audit;DROP FUNCTION ${fn}()`);}
 if(change==='expiry')await purchaseCorePeople(f.company);const result=await decideIamReview(raw,reviewer,reviewId,payload,command,(c,k,p)=>applyLifecycleReview(c,reviewer,k,p));expect(await decideIamReview(raw,reviewer,reviewId,payload,command)).toEqual(result);
});
it('People IAM rejects unregistered lifecycle review clients under enforcement',async()=>{
 const f=await fixture(),created=await peopleIamCase(f),client=await db.pool.connect();vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');try{await client.query('BEGIN');await expect(applyLifecycleReview(client,iamActorFromWork(f.owner),'people.lifecycle.activate',{caseId:created.id})).rejects.toMatchObject({code:'IAM_OWNER_COMPOSITION_REQUIRED'});}finally{await client.query('ROLLBACK');client.release();}
});

it.each(['hr_person','people_lifecycle_case'] as const)('People base Files keeps %s exact retained bytes and rolls final source and role changes back',async(targetType)=>{
 const {attachmentContent,bindRecordAttachmentSet}=await import('@erpx/server-files/public/attachments/service'),f=await fixture(),created=await peopleIamCase(f),targetId=targetType==='hr_person'?f.person:String(created.id),table=targetType==='hr_person'?'hr_people':'people_lifecycle_cases',version=Number(await raw.prepare(`SELECT version FROM ${table} WHERE id=?`).bind(targetId).first('version')),files=await seedAttachmentSet(raw,f.hr,{targetType,targetId,componentKind:'record',purposeKey:targetType==='hr_person'?'professional_certificate':'transfer_document',expectedTargetVersion:version}),objectKey=String(await raw.prepare('SELECT object_key FROM attachments WHERE id=?').bind(files.sourceId).first('object_key')),bytes='%PDF-1.7';await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:'application/pdf'}});await raw.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(files.sourceId).run();
 vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');await expect(attachmentContent(raw,env.H001_FILES,f.hr,files.linkId,'original')).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await expect(bindRecordAttachmentSet(raw,f.hr,files.setId,version,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await purchaseCorePeople(f.company);await purchaseCorePeople(f.company,'draining');
 const tables=[table,'assignments','attachments','attachment_sets','attachment_links','attachment_events','command_receipts','audit_events','outbox_events'],snapshot=()=>Promise.all(tables.map(async name=>(await raw.prepare(`SELECT row_to_json(t)::text row FROM ${name} t ORDER BY row_to_json(t)::text`).all()).results));
 for(const change of ['source','authority']){const command=key(),fn=`people_base_file_${key().replaceAll('-','')}`,before=await snapshot(),effect=change==='authority'?`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.hr.assignmentId}'`:targetType==='hr_person'?`UPDATE hr_people SET user_id='${f.owner.userId}' WHERE id='${targetId}'`:`UPDATE people_lifecycle_cases SET person_id='person-${f.owner.userId}' WHERE id='${targetId}'`;await raw.exec(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fn}()`);try{await expect(bindRecordAttachmentSet(raw,f.hr,files.setId,version,command)).rejects.toBeInstanceOf(Error);expect(await snapshot()).toEqual(before);}finally{await raw.exec(`DROP TRIGGER ${fn} ON audit_events;DROP FUNCTION ${fn}()`);}}
 const command=key(),bound=await bindRecordAttachmentSet(raw,f.hr,files.setId,version,command);expect(await bindRecordAttachmentSet(raw,f.hr,files.setId,version,command)).toEqual(bound);expect(await (await attachmentContent(raw,env.H001_FILES,f.hr,files.linkId,'original')).object.text()).toBe(bytes);await raw.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.hr.assignmentId).run();await expect(attachmentContent(raw,env.H001_FILES,f.hr,files.linkId,'original')).rejects.toBeInstanceOf(Error);expect(await raw.prepare('SELECT legal_hold,revoked_at FROM attachments WHERE id=?').bind(files.sourceId).first()).toEqual({legal_hold:1,revoked_at:null});expect(await (await env.H001_FILES.get(objectKey))!.text()).toBe(bytes);
});
it('People base projections gate actual private Records before limits and preserve unknown foundation references',async()=>{
 const {getRecord,listRecords,recordsPage,searchReferences}=await import('@erpx/server-records/public/records'),f=await fixture(),actor={...f.hr,modules:['people_payroll']},shared=key(),now=Date.now(),ids=[f.person,f.engagement];
 for(const [index,id]of [shared,...ids].entries())await raw.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,created_by,updated_by,created_at,updated_at) VALUES('people_payroll',?,'payroll-summaries',?,'','draft','[]',?,?,?,?,?)").bind(id,`People source projection ${index}`,f.company,f.hr.userId,f.hr.userId,now+index,now+index).run();
 vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');await expect(getRecord(actor,'people_payroll',f.person,raw)).rejects.toMatchObject({code:'PEOPLE_RECORD_NOT_FOUND'});expect((await listRecords(actor,'people_payroll','payroll-summaries')).map(row=>row.id)).toEqual([shared]);expect((await recordsPage(actor,'people_payroll','payroll-summaries',{q:'People source projection'})).items.map(row=>row.id)).toEqual([shared]);expect((await searchReferences(actor,'People source projection')).filter(row=>row.kind==='record').map(row=>'id'in row?row.id:null)).toEqual([shared]);
 await purchaseCorePeople(f.company);await purchaseCorePeople(f.company,'draining');expect((await listRecords(actor,'people_payroll','payroll-summaries')).map(row=>row.id).sort()).toEqual([shared,...ids].sort());const narrowed={...actor,iam:{...actor.iam!,permissions:['records.read']}};expect((await listRecords(narrowed,'people_payroll','payroll-summaries')).map(row=>row.id)).toEqual([shared]);await raw.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(actor.assignmentId).run();expect((await listRecords(actor,'people_payroll','payroll-summaries')).map(row=>row.id)).toEqual([shared]);
});
it('People base Work keeps actual foundation preparation and prevents incoming draining actions after Flow enrichment',async()=>{
 const f=await fixture(),input=lifecycleInput.parse({kind:'transfer',expectedVersion:0,personId:f.person,engagementId:f.engagement,positionId:f.position,managerPersonId:`person-${f.owner.userId}`,effectiveAt:Date.now()+60000,reason:'Private People Work source'}),preview=await previewLifecycle(raw,f.hr,input,true),created=await saveLifecycle(raw,f.hr,input,key(),true,preview.token,true);
 vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');expect(await peopleLifecycleWork(raw,f.hr)).toEqual([]);const foundationStore=packageGrantStore(raw,{authorize:async()=>({userId:'isolated-work-foundation-provider'}),obligations:async()=>{throw Error('No inactive witness');}});await foundationStore.apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:f.company,packageId:'projects',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Projects shared preparation foundation',evidence:'test:people-work-foundation'}});const foundationWork=await peopleLifecycleWork(raw,f.hr);expect(foundationWork.some(item=>item.id===`position-prep-${f.position}`)).toBe(true);expect(foundationWork.some(item=>item.id===created.id)).toBe(false);await purchaseCorePeople(f.company);expect((await peopleLifecycleWork(raw,f.hr)).some(item=>item.id===created.id)).toBe(true);await purchaseCorePeople(f.company,'draining');const item=(await peopleLifecycleWork(raw,f.hr)).find(item=>item.id===created.id)!;expect(item.canAct).toBe(false);const enriched=await enrichFlowWork(f.hr,[{...item,canAct:true,queue:'todo',relationships:['approval','assigned']}]);expect(enriched[0]).toMatchObject({id:created.id,canAct:false,queue:'waiting'});expect(enriched[0].relationships).toEqual([]);await raw.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.hr.assignmentId).run();expect(await enrichFlowWork(f.hr,enriched)).toEqual([]);
});
it('People base clearance preserves assigned notifications before limits and atomically closes retained evidence in its Calendar owner',async()=>{
 const {calendarCommand}=await import('@erpx/server-work_projection/public/calendar/commands'),{inboxPage,publishInboxEvent}=await import('@erpx/server-collaboration/public/inbox'),{listStaffNotifications}=await import('@erpx/server-collaboration/public/staff-notifications'),{attachmentContent}=await import('@erpx/server-files/public/attachments/service'),f=await fixture(),input=lifecycleInput.parse({kind:'offboarding',departureReason:'retirement',expectedVersion:0,personId:f.person,engagementId:f.engagement,positionId:null,lastWorkingDate:'2027-01-31',effectiveAt:Date.now()+86400000,reason:'Retained clearance owner purchase proof',clearanceOwners:{hr:f.hr.userId}}),created=await saveLifecycle(raw,f.hr,input,key(),true,(await previewLifecycle(raw,f.hr,input)).token),tasks=(await peopleLifecycleData(raw,f.hr)).tasks.filter(t=>t.case_id===created.id),own=tasks.find(t=>t.clearance_type==='hr')!,other=tasks.find(t=>t.clearance_type==='equipment')!,files=await seedAttachmentSet(raw,f.hr,{targetType:'people_clearance_task',targetId:own.id,componentKind:'action',componentRef:'hr',purposeKey:'clearance_evidence',expectedTargetVersion:1}),objectKey=String(await raw.prepare('SELECT object_key FROM attachments WHERE id=?').bind(files.sourceId).first('object_key')),bytes='%PDF-1.7';await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:'application/pdf'}});await raw.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(files.sourceId).run();
 const now=Date.now();for(let index=0;index<106;index++){const recordId=index===0?own.id:other.id,message=index===0?'Own retained People clearance':'Other clearance source';await raw.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'retirement_clearance',?,?,?,?)").bind(key(),f.company,f.hr.userId,recordId,index+100,message,now+index).run();await publishInboxEvent(raw,{key:key(),companyId:f.company,sourceType:'retirement_clearance',sourceId:recordId,sourceVersion:index+100,category:'work',preview:message,recipientIds:[f.hr.userId]});}
 const payload={action:'task.complete',id:own.id,expectedVersion:1,status:'completed',reason:'Reviewed retained clearance and exact evidence',attachmentSetIds:[files.setId]},command=key();vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');expect(await listStaffNotifications(raw,f.hr)).toEqual([]);expect((await inboxPage(raw,f.hr,{view:'all'})).items).toEqual([]);await expect(calendarCommand(raw,f.hr,payload,command)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await purchaseCorePeople(f.company);await purchaseCorePeople(f.company,'draining');
 for(const notices of [await listStaffNotifications(raw,f.hr),(await inboxPage(raw,f.hr,{view:'all'})).items]){expect(notices.some(n=>n.message==='Own retained People clearance')).toBe(true);expect(notices.some(n=>n.message==='Other clearance source')).toBe(false);}
 const fn=`people_calendar_${key().replaceAll('-','')}`,tables=['people_preparation_tasks','attachments','attachment_sets','attachment_links','attachment_events','command_receipts','audit_events','outbox_events'],snapshot=()=>Promise.all(tables.map(async table=>(await raw.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await snapshot();await raw.exec(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE people_preparation_tasks SET clearance_type='equipment' WHERE id='${own.id}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fn}()`);try{await expect(calendarCommand(raw,f.hr,payload,command)).rejects.toMatchObject({code:'PEOPLE_SOURCE_CHANGED'});expect(await snapshot()).toEqual(before);}finally{await raw.exec(`DROP TRIGGER ${fn} ON audit_events;DROP FUNCTION ${fn}()`);}
 const result=await calendarCommand(raw,f.hr,payload,command);expect(await calendarCommand(raw,f.hr,payload,command)).toEqual(result);expect(await attachmentBindingState(raw,files.setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:'retirement_clearance_completion',binding_id:own.id});expect(await(await attachmentContent(raw,env.H001_FILES,f.hr,files.linkId,'original')).object.text()).toBe(bytes);
});

const basePeopleSettlementInput=(companyId:string)=>({companyId,packageId:'people',packageVersion:1});
async function basePeopleSettlement(companyId:string){const {withOwnerPackageObligations,readOwnerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations');return withOwnerPackageObligations(raw,basePeopleSettlementInput(companyId),tx=>readOwnerPackageObligations(raw,tx,{...basePeopleSettlementInput(companyId),now:Date.now()}));}
async function basePeopleWithdrawal(companyId:string){const {packageGrantRevision}=await import('@erpx/server-commercial/public/package-grants'),grants=await readCompanyPackageGrants(raw,companyId),current=grants.find(row=>row.packageId==='people')!;return {companyId,expectedRevision:packageGrantRevision(companyId,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,state:'inactive' as const,version:current.version+1,reason:'Original People lifecycle duties discharged with standing employment retained',evidence:'test:people-base-owner-withdrawal'}}]};}
async function basePeopleWithdrawalReview(request:Awaited<ReturnType<typeof basePeopleWithdrawal>>,apply=false){const {withOwnerPackageObligations,ownerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),store=packageGrantStore(raw,{authorize:async()=>({userId:'isolated-people-base-settlement-provider'}),obligations:(tx,companyId,packageId)=>ownerPackageObligations(raw,tx,companyId,packageId)});return withOwnerPackageObligations(raw,basePeopleSettlementInput(request.companyId),tx=>apply?store.batch.applyIn(tx,request,async()=>{}):store.batch.previewIn(tx,request));}
it('settles actual base People lifecycle through normal Commercial review while preserving standing employment, identity, Legal-held private bytes and exact replay',async()=>{
 const {attachmentContent,bindRecordAttachmentSet}=await import('@erpx/server-files/public/attachments/service'),f=await fixture();await purchaseCorePeople(f.company);vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const input=lifecycleInput.parse({kind:'offboarding',departureReason:'retirement',expectedVersion:0,personId:f.person,engagementId:f.engagement,positionId:null,lastWorkingDate:'2027-01-31',effectiveAt:Date.now()+86400000,reason:'Original retained People case before software withdrawal'}),saved=await saveLifecycle(raw,f.hr,input,key(),false),id=String(saved.id);await purchaseCorePeople(f.company,'draining');expect((await basePeopleSettlement(f.company)).total).toBeGreaterThan(0);await expect(basePeopleWithdrawalReview(await basePeopleWithdrawal(f.company))).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');const command=key(),cancelled=await cancelLifecycle(raw,f.hr,id,1,command);
 const version=Number(await raw.prepare('SELECT version FROM hr_people WHERE id=?').bind(f.person).first('version')),files=await seedAttachmentSet(raw,f.hr,{targetType:'hr_person',targetId:f.person,componentKind:'record',purposeKey:'professional_certificate',expectedTargetVersion:version}),objectKey=String(await raw.prepare('SELECT object_key FROM attachments WHERE id=?').bind(files.sourceId).first('object_key')),bytes='%PDF-1.7\nOriginal retained private worker certificate';await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:'application/pdf'}});await bindRecordAttachmentSet(raw,f.hr,files.setId,version,key());await raw.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(files.sourceId).run();const before=await raw.prepare('SELECT id,status,version FROM hr_engagements WHERE company_id=? ORDER BY id').bind(f.company).all();expect((await basePeopleSettlement(f.company)).total).toBe(0);
 const request=await basePeopleWithdrawal(f.company),preview=await basePeopleWithdrawalReview(request),applied=await basePeopleWithdrawalReview(request,true);expect(applied).toEqual(preview);expect(await basePeopleWithdrawalReview(request,true)).toEqual(applied);expect(await cancelLifecycle(raw,f.hr,id,1,command)).toEqual(cancelled);expect((await raw.prepare('SELECT id,status,version FROM hr_engagements WHERE company_id=? ORDER BY id').bind(f.company).all()).results).toEqual(before.results);expect(await raw.prepare('SELECT status FROM hr_people WHERE id=?').bind(f.person).first('status')).toBe('active');expect(await(await attachmentContent(raw,env.H001_FILES,f.hr,files.linkId,'original')).object.text()).toBe(bytes);expect((await peopleLifecycleData(raw,f.hr)).cases.map(row=>row.id)).toContain(id);await expect(saveLifecycle(raw,f.hr,input,key(),false)).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});await raw.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.hr.assignmentId).run();await expect(cancelLifecycle(raw,f.hr,id,1,command)).rejects.toMatchObject({code:'PEOPLE_ACCESS_CHANGED'});await expect(attachmentContent(raw,env.H001_FILES,f.hr,files.linkId,'original')).rejects.toBeInstanceOf(Error);expect(await raw.prepare('SELECT legal_hold,revoked_at FROM attachments WHERE id=?').bind(files.sourceId).first()).toEqual({legal_hold:1,revoked_at:null});expect(await(await env.H001_FILES.get(objectKey))!.text()).toBe(bytes);
});
it('observes actual unfinished Benefits and employee expense owners for base People independently of purchases and refuses forged legacy monetary closure',async()=>{
 const f=await fixture();expect((await basePeopleSettlement(f.company)).total).toBe(0);const id=await f.claim(),withExpense=await basePeopleSettlement(f.company);expect(withExpense.observations.find(row=>row.moduleId==='finance')!.outstanding).toBeGreaterThan(0);await f.action(f.claimant,id,'cancel');expect((await basePeopleSettlement(f.company)).total).toBe(0);const plan=`people-benefit-${key()}`;await raw.prepare("INSERT INTO benefit_plans(id,company_id,name,provider,coverage_type,currency,employee_deduction_minor,employer_cost_minor,effective_from,created_by,created_at,updated_at) VALUES(?,?,'Actual unfinished employer benefit','Original provider','employee','THB',100,200,'2026-01-01',?,1,1)").bind(plan,f.company,f.hr.userId).run();const pending=await basePeopleSettlement(f.company);expect(pending.observations.find(row=>row.moduleId==='benefits')!.outstanding).toBe(1);expect(pending.observations.find(row=>row.moduleId==='finance')!.outstanding).toBe(0);expect(await raw.prepare('SELECT status FROM hr_engagements WHERE id=?').bind(f.engagement).first('status')).toBe('active');
});
