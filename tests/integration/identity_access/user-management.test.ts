import { beforeAll, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import { iamDb } from "../../../src/server/modules/identity_access/public/iam-db";
import { loadIamAuthority, iamRole } from "../../../src/server/modules/identity_access/public/iam-store";
import { selectWorkRole } from "../../../src/server/modules/work_projection/public/work-role-store";
import { captureJobAuthority } from "../../../src/server/modules/identity_access/public/iam-jobs";
import { decideIamReview } from "../../../src/server/modules/identity_access/public/iam-service";
import {
  applyManagementReview,
  managementCommand,
  readManagement,
  managementWork,
} from "../../../src/server/modules/identity_access/public/user-management";
import {
  attributeInput,
  validateAttributeValue,
  ruleMatches,
  type ManagementChange,
  type MembershipRule,
} from "../../../src/lib/identity_access/user-management";
import type { IamActor } from "../../../src/server/modules/identity_access/public/iam-context";
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({ runtimeEnv: async () => env }));
vi.mock("../../../src/server/modules/identity_access/public/iam-mfa", () => ({ requireFreshMfa: async () => {} }));
const db = iamDb(env.H001_DB),
  key = () => crypto.randomUUID();
beforeAll(async () => {
  await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS);
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
async function fixture() {
  const companyId = key();
  await db
    .prepare(
      "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Management test',?)",
    )
    .bind(companyId, companyId, companyId)
    .run();
  async function user(role = "holding_owner") {
    const userId = key(),
      assignmentId = key(),
      sessionId = key();
    await db
      .prepare(
        "INSERT INTO assignments(id,user_id,role,company_id,scope_kind,modules_json,valid_from,valid_until) VALUES(?,?,?,?,'holding','[]',0,?)",
      )
      .bind(assignmentId, userId, role, companyId, Date.now() + 200 * 86400000)
      .run();
    await selectWorkRole(db, {
      sessionId,
      userId,
      companyId,
      assignmentId,
      assignmentVersion: 1,
      revision: 0,
      expiresAt: Date.now() + 86400000,
    });
    return {
      userId,
      assignmentId,
      sessionId,
      companyId,
      role,
      holdingId: "h001",
      scopeKind: "holding",
      scopeNodeId: null,
      branchId: null,
      departmentId: null,
      platform: false,
      authority: { sessionId, revision: 1, assignmentVersion: 1 },
      iam: await loadIamAuthority(db, companyId, userId, role),
    } satisfies IamActor;
  }
  const owner = await user(),
    reviewer = await user(),
    it = await user("it_admin"),
    employee = await user("employee"),
    personId = key();
  await db
    .prepare(
      "INSERT INTO hr_people(id,company_id,user_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES(?,?,?,'Taylor Example','TH','active',?,?,?)",
    )
    .bind(
      personId,
      companyId,
      employee.userId,
      owner.userId,
      Date.now(),
      Date.now(),
    )
    .run();
  await db
    .prepare(
      "INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,'employee','salaried','active','2026-01-01','Engineer','THB',?,?,?)",
    )
    .bind(key(), personId, companyId, owner.userId, Date.now(), Date.now())
    .run();
  return { companyId, owner, reviewer, it, employee, personId };
}
async function change(actor: IamActor, change: ManagementChange) {
  const preview = await managementCommand(db, actor, change, key(), true);
  return managementCommand(
    db,
    actor,
    change,
    key(),
    false,
    preview.previewToken,
  );
}
it("lets Company IT edit person custom values without granting HR or definition authority", async () => {
  const f = await fixture();
  const field = await change(f.owner, { operation: "attribute", input: {
    label: "IT managed note", help: "", target: "person", type: "text", required: false,
    options: [], archived: false, readRoles: ["hr_admin"], writeRoles: ["hr_admin"],
    expectedVersion: 0, reason: "Track employee setup",
  } });
  const snapshot = await readManagement(db, f.it);
  expect(snapshot.canManagePeople).toBe(false);
  expect(snapshot.attributes.find(attribute => attribute.id === field.id)).toMatchObject({ canView: true, canEdit: true });
  await change(f.it, { operation: "values", input: { target: "person", objectId: f.personId,
    values: { [field.id]: "Assigned" }, expectedVersion: 0, reason: "Record setup" } });
  expect((await readManagement(db, f.it)).values.find(value => value.objectId === f.personId)?.values[field.id]).toBe("Assigned");
  await expect(change(f.it, { operation: "attribute", input: { label: "Forbidden field", help: "", target: "person", type: "text", required: false,
    options: [], archived: false, expectedVersion: 0, reason: "Definition change" } })).rejects.toMatchObject({ code: "PERMISSION_REQUIRED" });
  const other = await fixture();
  await expect(change(f.it, { operation: "values", input: { target: "person", objectId: other.personId,
    values: { [field.id]: "Outside" }, expectedVersion: 0, reason: "Cross Company" } })).rejects.toMatchObject({ code: "SCOPE_FORBIDDEN" });
});
const group = (
  name = "Engineers",
  rule: MembershipRule | null = null,
): ManagementChange => ({
  operation: "group",
  input: {
    name,
    kind: rule ? "automatic" : "manual",
    parentId: null,
    scopeNodeId: null,
    rule,
    archived: false,
    expectedVersion: 0,
    reason: "Manage test group",
  },
});
async function policy(actor: IamActor) {
  return change(actor, {
    operation: "policy",
    input: {
      name: "Read finance",
      expectedVersion: 0,
      reason: "Prepare scoped access",
      archived: false,
      grants: [
        {
          roleId: "auditor",
          expectedRoleRevision: (await iamRole(db,actor.companyId,"auditor"))!.revision,
          scopeKind: "holding",
          scopeNodeId: null,
          validUntil: Date.now() + 86400000,
        },
      ],
    },
  });
}
it("validates field types, false values, choices and real dates", () => {
  const base = {
    ...attributeInput.parse({
      label: "Review date",
      target: "person",
      type: "date",
      expectedVersion: 0,
      reason: "Create field",
    }),
    id: "date",
    version: 1,
  };
  expect(() => validateAttributeValue(base, "2026-02-31")).toThrow();
  expect(
    validateAttributeValue(
      { ...base, type: "checkbox", required: true },
      false,
    ),
  ).toBe(false);
  expect(() =>
    validateAttributeValue(
      { ...base, type: "single-select", options: ["A"] },
      "B",
    ),
  ).toThrow();
  expect(() =>
    validateAttributeValue({ ...base, type: "number" }, Infinity),
  ).toThrow();
});
it("keeps SQL and client rule semantics aligned, especially missing values", async () => {
  for (const condition of [
    { field: "level", op: "gte" as const, value: 2 },
    { field: "level", op: "neq" as const, value: 2 },
    { field: "tags", op: "contains" as const, value: "A" },
    { field: "missing", op: "empty" as const },
    { field: "missing", op: "neq" as const, value: "A" },
  ]) {
    const rule: MembershipRule = { mode: "all", conditions: [condition] },
      facts = { level: 3, tags: ["A", "B"] };
    const r = await db
      .prepare("SELECT um_rule_matches(?::jsonb,?::jsonb) ok")
      .bind(JSON.stringify(rule), JSON.stringify(facts))
      .first<{ ok: boolean }>();
    expect(r?.ok).toBe(ruleMatches(rule, facts));
  }
});
it("previews without persisting and rejects stale previews and cross-company objects", async () => {
  const f = await fixture(),
    other = await fixture(),
    input = group();
  const preview = await managementCommand(db, f.owner, input, key(), true);
  expect((await readManagement(db, f.owner)).groups).toHaveLength(0);
  await change(f.owner, group("Another group"));
  await expect(
    managementCommand(db, f.owner, input, key(), false, preview.previewToken),
  ).rejects.toThrow(/preview changed/i);
  const g = await change(other.owner, group());
  await expect(
    change(f.owner, {
      operation: "membership",
      input: {
        groupId: g.id,
        expectedVersion: 1,
        personIds: [f.personId],
        action: "add",
        reason: "Cannot cross companies",
      },
    }),
  ).rejects.toThrow();
  await expect(readManagement(db, f.employee)).rejects.toMatchObject({
    code: "PERMISSION_REQUIRED",
  });
});
it("supports nested membership, prevents cycles, and rejects manual additions to automatic groups", async () => {
  const f = await fixture(),
    parent = await change(f.owner, group("Parent"));
  const input = group("Child");
  if (input.operation !== "group") throw Error();
  const child = await change(f.owner, {
    ...input,
    input: { ...input.input, parentId: parent.id },
  });
  await change(f.owner, {
    operation: "membership",
    input: {
      groupId: child.id,
      expectedVersion: 1,
      personIds: [f.personId],
      action: "add",
      reason: "Add employee",
    },
  });
  const state = await readManagement(db, f.owner);
  expect(state.groups.find((g) => g.id === parent.id)?.members).toContain(
    f.personId,
  );
  const p = state.groups.find((g) => g.id === parent.id)!;
  await expect(
    change(f.owner, {
      operation: "group",
      input: {
        ...p,
        expectedVersion: p.version,
        parentId: child.id,
        reason: "Attempt cycle",
      },
    }),
  ).rejects.toThrow(/cycle/);
  const automatic = await change(
    f.owner,
    group("Auto", {
      mode: "all",
      conditions: [{ field: "job_title", op: "eq", value: "Engineer" }],
    }),
  );
  expect(
    (await readManagement(db, f.owner)).groups.find(
      (g) => g.id === automatic.id,
    )?.members,
  ).toEqual([f.personId]);
  await expect(
    change(f.owner, {
      operation: "membership",
      input: {
        groupId: automatic.id,
        expectedVersion: 1,
        personIds: [f.personId],
        action: "add",
        reason: "Invalid addition",
      },
    }),
  ).rejects.toThrow(/Automatic/);
});
it("requires independent approval, invalidates access synchronously, and creates a fresh proposal after rejoin", async () => {
  const f = await fixture(),
    g = await change(
      f.owner,
      group("Automatic engineers", {
        mode: "all",
        conditions: [{ field: "job_title", op: "eq", value: "Engineer" }],
      }),
    ),
    p = await policy(f.owner);
  await change(f.owner, {
    operation: "attachment",
    input: {
      policyId: p.id,
      target: "group",
      targetId: g.id,
      exclusions: [],
      archived: false,
      expectedVersion: 0,
      reason: "Attach access",
    },
  });
  let state = await readManagement(db, f.owner),
    candidate = state.candidates[0];
  expect(candidate.status).toBe("pending");
  expect(candidate.assignmentId).toBeNull();
  await expect(
    decideIamReview(
      db,
      f.owner,
      candidate.reviewId!,
      {
        expectedVersion: 1,
        decision: "approve",
        reason: "Cannot self approve",
      },
      key(),
      (c, _, p) => applyManagementReview(c, f.owner, p),
    ),
  ).rejects.toMatchObject({ code: "INDEPENDENT_REVIEW_REQUIRED" });
  await decideIamReview(
    db,
    f.reviewer,
    candidate.reviewId!,
    { expectedVersion: 1, decision: "approve", reason: "Independent review" },
    key(),
    (c, _, p) => applyManagementReview(c, f.reviewer, p),
  );
  candidate = (await readManagement(db, f.owner)).candidates[0];
  expect(
    (
      await db
        .prepare("SELECT iam_assignment_active(?,?) ok")
        .bind(candidate.assignmentId, Date.now())
        .first<{ ok: boolean }>()
    )?.ok,
  ).toBe(true);
  await db
    .prepare(
      "UPDATE hr_engagements SET job_title='Designer' WHERE company_id=? AND person_id=?",
    )
    .bind(f.companyId, f.personId)
    .run();
  expect(
    (
      await db
        .prepare("SELECT iam_assignment_active(?,?) ok")
        .bind(candidate.assignmentId, Date.now())
        .first<{ ok: boolean }>()
    )?.ok,
  ).toBe(false);
  expect((await readManagement(db, f.owner)).candidates[0].status).toBe(
    "revoked",
  );
  expect(
    (
      await db
        .prepare("SELECT iam_assignment_active(?,?) ok")
        .bind(f.employee.assignmentId, Date.now())
        .first<{ ok: boolean }>()
    )?.ok,
  ).toBe(true);
  await db
    .prepare(
      "UPDATE hr_engagements SET job_title='Engineer' WHERE company_id=? AND person_id=?",
    )
    .bind(f.companyId, f.personId)
    .run();
  await managementWork(db);
  state = await readManagement(db, f.owner);
  expect(state.candidates.filter((c) => c.status === "pending")).toHaveLength(
    1,
  );
  expect(
    state.candidates.find((c) => c.status === "pending")?.reviewId,
  ).not.toBe(candidate.reviewId);
});
it("keeps independent policy sources separate and denies expiry and stale revisions without a worker", async () => {
  const f = await fixture(),
    p = await policy(f.owner);
  for (const name of ["Source one", "Source two"]) {
    const g = await change(
      f.owner,
      group(name, {
        mode: "all",
        conditions: [{ field: "job_title", op: "eq", value: "Engineer" }],
      }),
    );
    await change(f.owner, {
      operation: "attachment",
      input: {
        policyId: p.id,
        target: "group",
        targetId: g.id,
        exclusions: [],
        archived: false,
        expectedVersion: 0,
        reason: "Attach independent source",
      },
    });
  }
  let state = await readManagement(db, f.owner);
  expect(state.candidates).toHaveLength(2);
  for (const candidate of state.candidates)
    await decideIamReview(
      db,
      f.reviewer,
      candidate.reviewId!,
      {
        expectedVersion: 1,
        decision: "approve",
        reason: "Review independent source",
      },
      key(),
      (c, _, payload) => applyManagementReview(c, f.reviewer, payload),
    );
  state = await readManagement(db, f.owner);
  const first = state.candidates[0],
    second = state.candidates[1];
  expect(first.assignmentId).not.toBe(second.assignmentId);
  const attachment = state.attachments.find(
    (a) => a.id === first.attachmentId,
  )!;
  await change(f.owner, {
    operation: "attachment",
    input: {
      ...attachment,
      expectedVersion: attachment.version,
      archived: true,
      reason: "Remove only one source",
    },
  });
  const active = async (id: string, time = Date.now()) =>
    (
      await db
        .prepare("SELECT iam_assignment_active(?,?) ok")
        .bind(id, time)
        .first<{ ok: boolean }>()
    )?.ok;
  expect(await active(first.assignmentId!)).toBe(false);
  expect(await active(second.assignmentId!)).toBe(true);
  expect(await active(second.assignmentId!, second.validUntil + 1)).toBe(false);
  // The database denies stale evidence even before reconciliation can run.
  await db
    .prepare(
      "UPDATE um_policies SET version=version+1 WHERE company_id=? AND id=?",
    )
    .bind(f.companyId, p.id)
    .run();
  expect(await active(second.assignmentId!)).toBe(false);
  expect(await active(f.employee.assignmentId)).toBe(true);
  await expect(
    captureJobAuthority(db, {
      ...f.employee,
      assignmentId: second.assignmentId!,
    }),
  ).rejects.toMatchObject({ code: "ASSIGNMENT_UNAVAILABLE" });
});
it("stores company-scoped attributes and prevents archiving a rule dependency", async () => {
  const f = await fixture();
  const a = await change(f.owner, {
    operation: "attribute",
    input: {
      label: "Clearance",
      help: "",
      type: "number",
      target: "person",
      required: false,
      options: [],
      archived: false,
      expectedVersion: 0,
      reason: "Track clearance",
    },
  });
  await change(f.owner, {
    operation: "values",
    input: {
      target: "person",
      objectId: f.personId,
      expectedVersion: 0,
      values: { [a.id]: 4 },
      reason: "Record clearance",
    },
  });
  const g = await change(
    f.owner,
    group("Cleared", {
      mode: "all",
      conditions: [{ field: "custom:" + a.id, op: "gte", value: 3 }],
    }),
  );
  const state = await readManagement(db, f.owner);
  expect(state.groups.find((v) => v.id === g.id)?.members).toEqual([
    f.personId,
  ]);
  const field = state.attributes[0];
  await expect(
    change(f.owner, {
      operation: "attribute",
      input: {
        ...field,
        expectedVersion: field.version,
        archived: true,
        reason: "Attempt archive",
      },
    }),
  ).rejects.toThrow(/active group rules/);
});

it("narrows organization attachments with exclusions and isolates delegated views", async () => {
  const f = await fixture(),
    branch = key(),
    department = key(),
    outside = key();
  for (const [id, kind, parent] of [
    [branch, "branch", f.companyId],
    [department, "department", branch],
    [outside, "branch", f.companyId],
  ])
    await db
      .prepare(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,?,?,?,?,?,?)",
      )
      .bind(
        id,
        kind,
        id,
        id,
        parent,
        f.companyId,
        kind === "department" ? branch : null,
      )
      .run();
  await db
    .prepare(
      "UPDATE hr_engagements SET branch_id=?,department_id=? WHERE company_id=? AND person_id=?",
    )
    .bind(branch, department, f.companyId, f.personId)
    .run();
  const p = await policy(f.owner);
  const attach = await change(f.owner, {
    operation: "attachment",
    input: {
      policyId: p.id,
      target: "unit",
      targetId: branch,
      exclusions: [department],
      archived: false,
      expectedVersion: 0,
      reason: "Exclude department",
    },
  });
  expect((await readManagement(db, f.owner)).candidates).toHaveLength(0);
  const limited = {
    ...f.owner,
    scopeKind: "branch",
    scopeNodeId: outside,
    branchId: outside,
  };
  expect((await readManagement(db, limited)).people).toHaveLength(0);
  await expect(
    managementCommand(
      db,
      limited,
      {
        operation: "values",
        input: {
          target: "person",
          objectId: f.personId,
          expectedVersion: 0,
          values: {},
          reason: "Attempt outside edit",
        },
      },
      key(),
      true,
    ),
  ).rejects.toMatchObject({ code: "SCOPE_FORBIDDEN" });
  const a = (await readManagement(db, f.owner)).attachments.find(
    (a) => a.id === attach.id,
  )!;
  await change(f.owner, {
    operation: "attachment",
    input: {
      ...a,
      expectedVersion: a.version,
      exclusions: [],
      reason: "Include department",
    },
  });
  const candidate = (await readManagement(db, f.owner)).candidates[0];
  await decideIamReview(
    db,
    f.reviewer,
    candidate.reviewId!,
    { expectedVersion: 1, decision: "approve", reason: "Review inclusion" },
    key(),
    (c, _, p) => applyManagementReview(c, f.reviewer, p),
  );
  const approved = (await readManagement(db, f.owner)).candidates[0];
  await captureJobAuthority(db, {
    ...f.employee,
    assignmentId: approved.assignmentId!,
  });
  await db
    .prepare(
      "UPDATE hr_engagements SET branch_id=?,department_id=NULL WHERE company_id=? AND person_id=?",
    )
    .bind(outside, f.companyId, f.personId)
    .run();
  await expect(
    captureJobAuthority(db, {
      ...f.employee,
      assignmentId: approved.assignmentId!,
    }),
  ).rejects.toMatchObject({ code: "ASSIGNMENT_UNAVAILABLE" });
});

it("rejects stale beneficiary approvals and retains rejected decisions until eligibility changes", async () => {
  const f = await fixture(),
    g = await change(
      f.owner,
      group("Membership rule", {
        mode: "all",
        conditions: [{ field: "job_title", op: "eq", value: "Engineer" }],
      }),
    ),
    p = await policy(f.owner);
  await change(f.owner, {
    operation: "attachment",
    input: {
      policyId: p.id,
      target: "group",
      targetId: g.id,
      exclusions: [],
      archived: false,
      expectedVersion: 0,
      reason: "Attach rule policy",
    },
  });
  const candidate = (await readManagement(db, f.owner)).candidates[0];
  await decideIamReview(
    db,
    f.reviewer,
    candidate.reviewId!,
    { expectedVersion: 1, decision: "reject", reason: "Not appropriate yet" },
    key(),
  );
  await managementWork(db);
  await managementWork(db);
  expect((await readManagement(db, f.owner)).candidates).toHaveLength(1);
  await db
    .prepare(
      "UPDATE hr_engagements SET job_title='Other' WHERE person_id=? AND company_id=?",
    )
    .bind(f.personId, f.companyId)
    .run();
  await db
    .prepare(
      "UPDATE hr_engagements SET job_title='Engineer' WHERE person_id=? AND company_id=?",
    )
    .bind(f.personId, f.companyId)
    .run();
  await managementWork(db);
  const next = (await readManagement(db, f.owner)).candidates.find(
    (c) => c.status === "pending",
  )!;
  expect(next.reviewId).not.toBe(candidate.reviewId);
  await db
    .prepare(
      "UPDATE hr_engagements SET job_title='Other' WHERE person_id=? AND company_id=?",
    )
    .bind(f.personId, f.companyId)
    .run();
  await expect(
    decideIamReview(
      db,
      f.reviewer,
      next.reviewId!,
      { expectedVersion: 1, decision: "approve", reason: "Stale review" },
      key(),
      (c, _, p) => applyManagementReview(c, f.reviewer, p),
    ),
  ).rejects.toMatchObject({ code: "REVIEW_CHANGED" });
});

it("keeps previews retryable, permits clearing optional attributes, and rejects protected policy roles", async () => {
  const f = await fixture();
  const a = await change(f.owner, {
    operation: "attribute",
    input: {
      label: "Optional number",
      help: "",
      target: "person",
      type: "number",
      required: false,
      options: [],
      archived: false,
      expectedVersion: 0,
      reason: "Optional metadata",
    },
  });
  await change(f.owner, {
    operation: "values",
    input: {
      target: "person",
      objectId: f.personId,
      values: { [a.id]: 2 },
      expectedVersion: 0,
      reason: "Set value",
    },
  });
  await change(f.owner, {
    operation: "values",
    input: {
      target: "person",
      objectId: f.personId,
      values: { [a.id]: null },
      expectedVersion: 1,
      reason: "Clear value",
    },
  });
  expect((await readManagement(db, f.owner)).values[0].values[a.id]).toBeNull();
  const input = group("Idempotent"),
    preview = await managementCommand(db, f.owner, input, key(), true),
    commandKey = key();
  const beforeHistory = await db.prepare("SELECT count(*) count FROM um_history WHERE company_id=?").bind(f.companyId).first<{count:number}>();
  await db.exec(`CREATE FUNCTION fail_management_audit_test() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.target_id='${commandKey}' THEN RAISE EXCEPTION 'forced final management audit failure' USING ERRCODE='23514'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_management_audit_test BEFORE INSERT ON iam_audit FOR EACH ROW EXECUTE FUNCTION fail_management_audit_test()`);
  try {
    await expect(managementCommand(db,f.owner,input,commandKey,false,preview.previewToken)).rejects.toMatchObject({code:'23514'});
  } finally {
    await db.exec('DROP TRIGGER fail_management_audit_test ON iam_audit; DROP FUNCTION fail_management_audit_test()');
  }
  expect(await db.prepare('SELECT id FROM iam_receipts WHERE id=?').bind(commandKey).first()).toBeNull();
  expect(await db.prepare('SELECT id FROM iam_audit WHERE target_id=?').bind(commandKey).first()).toBeNull();
  expect(await db.prepare("SELECT id FROM um_groups WHERE company_id=? AND definition->>'name'='Idempotent'").bind(f.companyId).first()).toBeNull();
  expect(await db.prepare('SELECT count(*) count FROM um_history WHERE company_id=?').bind(f.companyId).first()).toEqual(beforeHistory);
  const first = await managementCommand(
    db,
    f.owner,
    input,
    commandKey,
    false,
    preview.previewToken,
  );
  expect(
    await managementCommand(
      db,
      f.owner,
      input,
      commandKey,
      false,
      preview.previewToken,
    ),
  ).toEqual(first);
  expect(await db.prepare('SELECT count(*) count FROM iam_receipts WHERE id=?').bind(commandKey).first()).toEqual({count:1});
  expect(await db.prepare('SELECT count(*) count FROM iam_audit WHERE target_id=?').bind(commandKey).first()).toEqual({count:1});
  expect(await db.prepare("SELECT count(*) count FROM um_groups WHERE company_id=? AND definition->>'name'='Idempotent'").bind(f.companyId).first()).toEqual({count:1});
  await expect(
    change(f.owner, {
      operation: "policy",
      input: {
        name: "Invalid owners",
        expectedVersion: 0,
        reason: "Reject automated ownership",
        archived: false,
        grants: [
          {
            roleId: "holding_owner",
            expectedRoleRevision: (await iamRole(db,f.owner.companyId,"holding_owner"))!.revision,
            scopeKind: "holding",
            scopeNodeId: null,
            validUntil: Date.now() + 86400000,
          },
        ],
      },
    }),
  ).rejects.toThrow(/delegated permissions|direct assignments/);
});
