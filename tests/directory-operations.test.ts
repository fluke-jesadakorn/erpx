import { beforeAll, beforeEach, expect, it, vi } from "vitest";
vi.mock("server-only", () => ({}));
import { env, applyMigrations } from "./runtime-env";
import {
  expenseFixture,
  expenseDb as raw,
  key,
} from "./helpers/expense-fixture";
import { iamDb } from "../src/server/iam-db";
import { changeAppointment } from "../src/server/department-authority";
import { iamActorFromWork, type IamActor } from "../src/server/iam-context";
import { selectWorkRole } from "../src/server/work-role-store";
import { loadIamAuthority } from "../src/server/iam-store";
import type { WorkContext } from "../src/server/authorization";
import { decideIamReview } from "../src/server/iam-service";
import {
  readManagement,
  managementCommand,
} from "../src/server/user-management";
import { attributeInput } from "../src/lib/user-management";
import {
  saveHoldingTemplate,
  adoptHoldingTemplate,
  peopleTemplatesData,
} from "../src/server/people-templates";
import { mappedFields } from "../src/server/directory-field-mapping";
import {
  configureDirectoryAutomation,
  refreshDirectoryInventory,
  previewDirectoryChange,
  submitDirectoryChange,
  applyDirectoryOperationReview,
  cancelDirectoryOperation,
  drainDirectoryOperations,
  directoryAutomationData,
  affectedObjects,
} from "../src/server/directory-operations";
import {
  directoryChangeInput,
  windowsPolicyDefinition,
  compileAdministrativePolicies,
  type DirectoryInventory,
} from "../src/lib/directory-operations";
import { compilePreferences } from "../src/lib/windows-preferences";
import {
  canonicalInventory,
  validateObjectTarget,
} from "../src/server/directory-provider";
const provider = vi.hoisted(() => ({ read: vi.fn(), apply: vi.fn() }));
vi.mock("../src/server/directory-provider", async (importOriginal) => ({
  ...(await importOriginal<
    typeof import("../src/server/directory-provider")
  >()),
  readAutomationInventory: provider.read,
  applyDirectoryAction: provider.apply,
}));
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
const inventory: DirectoryInventory = {
  providerVersion: "UDM fixture",
  observedAt: 1700000000000,
  complete: true,
  fields: [{ name: "firstname", type: "text", read: true, write: true }],
  objects: [
    {
      id: "ou",
      dn: "ou=Test,dc=example,dc=test",
      kind: "ou",
      name: "Test",
      parentId: null,
      version: "1",
      module: "container/ou",
      properties: {},
      memberIds: [],
    },
    {
      id: "user",
      dn: "uid=test,ou=Test,dc=example,dc=test",
      kind: "account",
      name: "Test user",
      parentId: "ou",
      version: "2",
      module: "users/user",
      properties: { firstname: "Test", disabled: false },
      memberIds: [],
    },
    {
      id: "group",
      dn: "cn=Staff,ou=Test,dc=example,dc=test",
      kind: "group",
      name: "Staff",
      parentId: "ou",
      version: "3",
      module: "groups/group",
      properties: {},
      memberIds: [],
    },
    {
      id: "child",
      dn: "cn=Child,ou=Test,dc=example,dc=test",
      kind: "group",
      name: "Child",
      parentId: "ou",
      version: "4",
      module: "groups/group",
      properties: {},
      memberIds: [],
    },
  ],
};
beforeEach(() => {
  provider.read
    .mockReset()
    .mockImplementation(async () => structuredClone(inventory));
  provider.apply.mockReset().mockResolvedValue({
    verified: true,
    objectId: "user",
    version: "verified",
  });
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
  return iamActorFromWork({
    ...a,
    authority: {
      sessionId,
      revision: 1,
      assignmentVersion: 1,
      expiresAt: Date.now() + 86400000,
    },
    iam: await loadIamAuthority(db, a.companyId, a.userId, a.role),
  } as WorkContext);
}
async function fixture() {
  const f = await expenseFixture(false),
    it = await authorize(f.it),
    owner = await authorize(f.owner),
    hrMember = await f.member("HR", "hr_admin"),
    connectionId = key();
  await changeAppointment(raw, owner, {kind:"hr",departmentId:null,positionId:null,personId:`person-${hrMember.userId}`,expectedVersion:0,reason:"Appoint fixture HR reviewer"}, key());
  const hr = await authorize(hrMember);
  await db.pool.query(
    "INSERT INTO iam_connections(id,company_id,name,provider,config,status,version,created_by,created_at) VALUES($1,$2,'Test UCS','univention',$3,'active',1,$4,$5)",
    [
      connectionId,
      f.company,
      JSON.stringify({
        mode: "ldaps",
        authority: key(),
        secretProfile: "fixture",
        baseDn: "dc=example,dc=test",
        ldapUrl: "ldaps://example.test",
      }),
      it.userId,
      Date.now(),
    ],
  );
  await refreshDirectoryInventory(db, it, connectionId, 1, key());
  const input = directoryChangeInput.parse({
    connectionId,
    expectedVersion: 1,
    action: {
      kind: "account.update",
      objectId: "user",
      fields: { firstname: "Updated" },
    },
    reason: "Update the reviewed employee record",
  });
  return { ...f, it, owner, hr, connectionId, input };
}
async function enable(f: Awaited<ReturnType<typeof fixture>>) {
  await configureDirectoryAutomation(
    db,
    f.it,
    { connectionId: f.connectionId, expectedVersion: 1, writeEnabled: true },
    key(),
  );
}
async function propose(
  f: Awaited<ReturnType<typeof fixture>>,
  input = f.input,
) {
  const p = await previewDirectoryChange(db, f.it, input);
  return submitDirectoryChange(db, f.it, input, p.token, key());
}
async function approve(actor: IamActor, id: string) {
  return decideIamReview(
    db,
    actor,
    id,
    {
      decision: "approve",
      expectedVersion: 1,
      reason: "Verified the directory impact",
    },
    key(),
    (c, kind, p) => applyDirectoryOperationReview(c, actor, p),
  );
}
it("requires tested write-back and independent review, preserves request idempotency, and verifies only after execution", async () => {
  const f = await fixture(),
    p = await previewDirectoryChange(db, f.it, f.input);
  await expect(
    submitDirectoryChange(db, f.it, f.input, p.token, key()),
  ).rejects.toMatchObject({ code: "DIRECTORY_READ_ONLY" });
  await enable(f);
  const fresh = await previewDirectoryChange(db, f.it, f.input),
    command = key(),
    job = await submitDirectoryChange(db, f.it, f.input, fresh.token, command);
  expect(
    await submitDirectoryChange(db, f.it, f.input, fresh.token, command),
  ).toEqual(job);
  await drainDirectoryOperations(db);
  expect(provider.apply).not.toHaveBeenCalled();
  await expect(approve(f.it, job.reviewId)).rejects.toMatchObject({
    code: "PERMISSION_REQUIRED",
  });
  await approve(f.owner, job.reviewId);
  await drainDirectoryOperations(db);
  await drainDirectoryOperations(db);
  expect(provider.apply, JSON.stringify((await directoryAutomationData(db,f.it)).jobs)).toHaveBeenCalledTimes(1);
  expect(
    (await directoryAutomationData(db, f.it)).jobs.find((j) => j.id === job.id)
      ?.status,
  ).toBe("completed");
});
it("rejects stale preview, cross-Company connection use and unsupported writable fields", async () => {
  const f = await fixture();
  await enable(f);
  const p = await previewDirectoryChange(db, f.it, f.input);
  await db.pool.query(
    "UPDATE people_directory_state SET inventory=jsonb_set(inventory,'{objects,1,version}','\"new\"') WHERE connection_id=$1",
    [f.connectionId],
  );
  await expect(
    submitDirectoryChange(db, f.it, f.input, p.token, key()),
  ).rejects.toMatchObject({ code: "DIRECTORY_PREVIEW_CHANGED" });
  await expect(
    previewDirectoryChange(db, { ...f.it, companyId: key() }, f.input),
  ).rejects.toMatchObject({ code: "CONNECTION_UNAVAILABLE" });
  await expect(
    previewDirectoryChange(db, f.it, {
      ...f.input,
      action: {
        kind: "account.update",
        objectId: "user",
        fields: { password: "forbidden" },
      },
    }),
  ).rejects.toMatchObject({ code: "DIRECTORY_FIELD_UNSUPPORTED" });
});
it("stops queued work when the authorizing assignment or write-back setting is withdrawn", async () => {
  const f = await fixture();
  await enable(f);
  const job = await propose(f);
  await approve(f.owner, job.reviewId);
  await db.pool.query(
    "UPDATE assignments SET revoked_at=$1,version=version+1 WHERE id=$2",
    [Date.now(), f.it.assignmentId],
  );
  await drainDirectoryOperations(db);
  expect(provider.apply).not.toHaveBeenCalled();
  expect(
    (
      await db.pool.query(
        "SELECT status,last_error FROM people_directory_operations WHERE id=$1",
        [job.id],
      )
    ).rows[0],
  ).toEqual({ status: "blocked", last_error: "DIRECTORY_AUTHORITY_CHANGED" });
});
it("cancels queued work and keeps completed or possibly applied work out of cancellation", async () => {
  const f = await fixture();
  await enable(f);
  const job = await propose(f);
  await cancelDirectoryOperation(db, f.it, job.id, 1, key());
  await drainDirectoryOperations(db);
  expect(provider.apply).not.toHaveBeenCalled();
  const second = await propose(f);
  await approve(f.owner, second.reviewId);
  await drainDirectoryOperations(db);
  await expect(
    cancelDirectoryOperation(db, f.it, second.id, 4, key()),
  ).rejects.toMatchObject({ code: "DIRECTORY_CANCEL_UNAVAILABLE" });
});
it("requires all affected Companies to be acknowledged for whole-account disable and blocks protected accounts", async () => {
  const f = await fixture();
  await enable(f);
  await db.pool.query(
    "INSERT INTO iam_identities(id,company_id,connection_id,external_id,username,name,enabled,observed_at,user_id) VALUES($1,$2,$3,'user','test','Test',true,$4,$5)",
    [key(), f.company, f.connectionId, Date.now(), f.owner.userId],
  );
  const input = {
    ...f.input,
    action: {
      kind: "account.disable" as const,
      objectId: "user",
      caseId: null,
    },
  };
  await expect(previewDirectoryChange(db, f.it, input)).rejects.toMatchObject({
    code: "PROTECTED_DIRECTORY_ACCOUNT",
  });
  await db.pool.query(
    "UPDATE iam_identities SET user_id=NULL WHERE connection_id=$1",
    [f.connectionId],
  );
  const p = await previewDirectoryChange(db, f.it, input);
  await expect(
    submitDirectoryChange(db, f.it, input, p.token, key()),
  ).rejects.toMatchObject({ code: "DIRECTORY_IMPACT_ACK_REQUIRED" });
});
it("rejects nested cycles and requires an explicit computer target", () => {
  const inv = structuredClone(inventory);
  inv.objects.find((o) => o.id === "group")!.memberIds = ["child"];
  expect(() =>
    validateObjectTarget(
      {
        kind: "group.members",
        objectId: "child",
        memberIds: ["group"],
        action: "add",
      },
      inv,
    ),
  ).toThrow("cycle");
  expect(
    windowsPolicyDefinition.safeParse({
      name: "USB policy",
      scope: "computer",
      targetOuId: "ou",
      filterGroupId: "group",
    }).success,
  ).toBe(false);
});
it("preserves UCS canonical identities while retaining the Samba write identity", () => {
  const source = structuredClone(inventory);
  source.objects[0].id = "samba-ou";
  source.objects[1].id = "samba-user";
  source.objects[1].parentId = "samba-ou";
  source.objects[2].memberIds = ["samba-user"];
  const migrated = canonicalInventory(source, {
    "samba-ou": "ou",
    "samba-user": "user",
  });
  expect(migrated.objects[1]).toMatchObject({
    id: "user",
    parentId: "ou",
    properties: {
      windowsIdentity: { id: "samba-user", version: "2" },
    },
  });
  expect(migrated.objects[2].memberIds).toEqual(["user"]);
  expect(() =>
    canonicalInventory(structuredClone(source), {
      "samba-ou": "same",
      "samba-user": "same",
    }),
  ).toThrow("one-to-one");
});
it("patches existing GPO settings without requiring replacement links or filters and previews all filter members", () => {
  const inv = structuredClone(inventory);
  inv.objects.push({id:"gpo",dn:"cn=Policy,cn=Policies,dc=example,dc=test",name:"Policy",kind:"gpo",parentId:null,module:"samba",version:"1",memberIds:[],properties:{securityFilterGroupIds:["group"]}});
  inv.objects.find(o=>o.id==="group")!.memberIds=["user"];
  const action = {kind:"gpo.publish" as const,objectId:"gpo",definition:windowsPolicyDefinition.parse({name:"Policy",scope:"user",targetOuId:"unselected",filterGroupId:"unselected",lockSeconds:600})};
  expect(()=>validateObjectTarget(action,inv)).not.toThrow();
  expect(affectedObjects(action,inv).map(o=>o.id)).toEqual(expect.arrayContaining(["gpo","group","user"]));
  expect(()=>validateObjectTarget({...action,definition:{...action.definition,scope:"computer",computerTargetId:null}},inv)).toThrow("explicit computer");
});
it("compiles registry policies and preferences separately and escapes preference XML without storing credentials", () => {
  const p = windowsPolicyDefinition.parse({
    name: "Office policy",
    scope: "user",
    targetOuId: "ou",
    filterGroupId: "group",
    usb: "deny",
    lockSeconds: 300,
    wallpaper: "\\\\server\\share\\wallpaper.jpg",
    drives: [
      {
        id: key(),
        letter: "S",
        path: "\\\\server\\share",
        label: 'R&D \"team\"',
      },
    ],
    printers: [{ id: key(), path: "\\\\server\\printer", isDefault: true }],
  });
  const registry = compileAdministrativePolicies(p),
    files = compilePreferences(p, "2026-09-17 00:00:00");
  expect(registry.some((r) => r.name === "Deny_All" && r.value === 1)).toBe(
    true,
  );
  expect(
    registry.some((r) => r.name === "ScreenSaveTimeOut" && r.value === "300"),
  ).toBe(true);
  expect(files).toHaveLength(2);
  expect(files[0].xml).toContain("R&amp;D &quot;team&quot;");
  expect(files[0].xml).not.toContain("cpassword");
  expect(files[1].xml).toContain('default="1"');
});
it("enforces field ACLs in snapshots and API mutations, including Branch overrides", async () => {
  const f = await fixture();
  const change = {
    operation: "attribute" as const,
    input: attributeInput.parse({
      label: "Private HR code",
      target: "person",
      type: "text",
      expectedVersion: 0,
      reason: "Restrict the HR-owned field",
      ownerRole: "hr_admin",
      readRoles: ["hr_admin"],
      writeRoles: ["hr_admin"],
      branchOverride: false,
    }),
  };
  const p = await managementCommand(db, f.owner, change, key(), true);
  const a = await managementCommand(
    db,
    f.owner,
    change,
    key(),
    false,
    p.previewToken,
  );
  const person = String(
    (
      await db.pool.query(
        "SELECT id FROM hr_people WHERE company_id=$1 AND user_id=$2",
        [f.company, f.hr.userId],
      )
    ).rows[0]?.id,
  );
  const values = {
    operation: "values",
    input: {
      target: "person",
      objectId: person,
      expectedVersion: 0,
      values: { [a.id]: "HR-PRIVATE" },
      reason: "Record HR data",
    },
  };
  const vp = await managementCommand(db, f.hr, values, key(), true);
  await managementCommand(db, f.hr, values, key(), false, vp.previewToken);
  const itSnapshot = await readManagement(db, f.it);
  expect(JSON.stringify(itSnapshot)).not.toContain("HR-PRIVATE");
  expect(itSnapshot.attributes.some((x) => x.id === a.id)).toBe(false);
  const asBranch = {
    ...f.hr,
    scopeKind: "branch",
    scopeNodeId: f.branch,
    branchId: f.branch,
  };
  await expect(
    managementCommand(
      db,
      asBranch,
      { ...values, input: { ...values.input, expectedVersion: 1 } },
      key(),
      true,
    ),
  ).rejects.toMatchObject({ code: "ATTRIBUTE_WRITE_FORBIDDEN" });
});
it("copies revisioned Holding fields and tasks without changing Company copies on publication", async () => {
  const f = await fixture();
  const definition = {
    onboardingTasks: ["Prepare the branch desk"],
    offboardingTasks: ["Return the laptop"],
    attributes: [
      {
        key: "desk",
        label: "Desk code",
        target: "person",
        type: "text",
        options: [],
        ownerRole: "hr_admin",
        readRoles: ["hr_admin"],
        writeRoles: ["hr_admin"],
        branchOverride: false,
      },
    ],
  };
  const t = await saveHoldingTemplate(
    db,
    f.owner,
    {
      expectedVersion: 0,
      name: "Shared preparation",
      definition,
      reason: "Publish Holding standard",
    },
    key(),
  );
  const input = {
    templateId: t.id,
    revision: 1,
    reason: "Adopt Company preparation",
  };
  const p = await adoptHoldingTemplate(db, f.owner, input, key(), true);
  await adoptHoldingTemplate(
    db,
    f.owner,
    { ...input, previewToken: p.previewToken },
    key(),
    false,
  );
  await saveHoldingTemplate(
    db,
    f.owner,
    {
      id: t.id,
      expectedVersion: 1,
      name: "Shared preparation",
      definition: {
        ...definition,
        onboardingTasks: ["Revised desk preparation"],
      },
      reason: "Revise future adoption",
    },
    key(),
  );
  expect(
    (
      await db.pool.query(
        "SELECT definition FROM people_lifecycle_settings WHERE company_id=$1",
        [f.company],
      )
    ).rows[0].definition.onboardingTasks,
  ).toEqual(definition.onboardingTasks);
  const templates = await peopleTemplatesData(db, f.owner);
  expect(templates.templates.find((x) => x.id === t.id)).toMatchObject({
    version: 2,
    adoptedVersion: 1,
  });
  await expect(
    adoptHoldingTemplate(
      db,
      f.owner,
      { ...input, previewToken: p.previewToken },
      key(),
      false,
    ),
  ).rejects.toMatchObject({ code: "TEMPLATE_CHANGED" });
});
it("requires an explicit verified identity before importing or exporting mapped fields", async () => {
  const f = await fixture();
  const person = (
    await db.pool.query(
      "SELECT id FROM hr_people WHERE company_id=$1 AND user_id=$2",
      [f.company, f.hr.userId],
    )
  ).rows[0].id;
  for (const direction of ["read", "write"] as const)
    await expect(
      mappedFields(
        db,
        f.owner,
        {
          connectionId: f.connectionId,
          personId: person,
          direction,
          reason: "Synchronize approved fields",
        },
        key(),
        true,
      ),
    ).rejects.toMatchObject({ code: "VERIFIED_IDENTITY_REQUIRED" });
  await expect(
    previewDirectoryChange(db, f.it, {
      ...f.input,
      action: { kind: "account.enable", objectId: "user", caseId: key() },
    }),
  ).rejects.toMatchObject({ code: "VERIFIED_LIFECYCLE_REQUIRED" });
});
it("inherits unit fields and previews resetting a Company-owned Branch override", async () => {
  const f = await fixture(),
    node = (
      await db.pool.query(
        "SELECT id,parent_id FROM organization_nodes WHERE company_id=$1 AND kind='branch' AND parent_id IS NOT NULL LIMIT 1",
        [f.company],
      )
    ).rows[0];
  expect(node).toBeTruthy();
  const change = {
    operation: "attribute",
    input: attributeInput.parse({
      label: "Branch printer",
      target: "unit",
      type: "text",
      expectedVersion: 0,
      reason: "Configure inherited printer",
      branchOverride: true,
    }),
  };
  const ap = await managementCommand(db, f.owner, change, key(), true),
    field = await managementCommand(
      db,
      f.owner,
      change,
      key(),
      false,
      ap.previewToken,
    );
  async function set(objectId: string, value: string, version = 0) {
    const body = {
        operation: "values",
        input: {
          target: "unit",
          objectId,
          expectedVersion: version,
          values: { [field.id]: value },
          reason: "Set the inherited printer",
        },
      },
      p = await managementCommand(db, f.owner, body, key(), true);
    return managementCommand(db, f.owner, body, key(), false, p.previewToken);
  }
  await set(node.parent_id, "Main printer");
  expect(
    (await readManagement(db, f.owner)).values.find(
      (v) => v.objectId === node.id,
    )?.values[field.id],
  ).toBe("Main printer");
  await set(node.id, "Branch printer");
  const body = {
      operation: "values",
      input: {
        target: "unit",
        objectId: node.id,
        expectedVersion: 1,
        values: {},
        inherit: [field.id],
        reason: "Use the Company default",
      },
    },
    p = await managementCommand(db, f.owner, body, key(), true);
  await managementCommand(db, f.owner, body, key(), false, p.previewToken);
  expect(
    (await readManagement(db, f.owner)).values.find(
      (v) => v.objectId === node.id,
    )?.values[field.id],
  ).toBe("Main printer");
});
it("imports supported fields for a reviewed identity and rejects a changed mapping preview", async () => {
  const f = await fixture(),
    person = (
      await db.pool.query(
        "SELECT id FROM hr_people WHERE company_id=$1 AND user_id=$2",
        [f.company, f.hr.userId],
      )
    ).rows[0].id;
  const identity = key();
  await db.pool.query(
    "INSERT INTO iam_identities(id,company_id,connection_id,external_id,username,name,enabled,observed_at,user_id) VALUES($1,$2,$3,'user','test','Test',true,$4,$5)",
    [identity, f.company, f.connectionId, Date.now(), f.hr.userId],
  );
  await db.pool.query(
    "INSERT INTO iam_members(company_id,user_id,principal_id,person_id,identity_id,connection_id) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(company_id,user_id) DO UPDATE SET person_id=excluded.person_id,identity_id=excluded.identity_id,connection_id=excluded.connection_id",
    [
      f.company,
      f.hr.userId,
      "local:" + f.hr.userId,
      person,
      identity,
      f.connectionId,
    ],
  );
  const definition = {
    operation: "attribute",
    input: attributeInput.parse({
      label: "Directory first name",
      target: "person",
      type: "text",
      expectedVersion: 0,
      reason: "Map the directory first name",
    }),
  };
  const preview = await managementCommand(db, f.owner, definition, key(), true),
    field = await managementCommand(
      db,
      f.owner,
      definition,
      key(),
      false,
      preview.previewToken,
    );
  await configureDirectoryAutomation(
    db,
    f.owner,
    {
      connectionId: f.connectionId,
      expectedVersion: 1,
      writeEnabled: false,
      mappings: [
        { attributeId: field.id, field: "firstname", direction: "read" },
      ],
    },
    key(),
  );
  const input = {
    connectionId: f.connectionId,
    personId: person,
    direction: "read" as const,
    reason: "Import reviewed directory name",
  };
  const first = (await mappedFields(db, f.owner, input, key(), true)) as {
    previewToken: string;
    changes: unknown[];
  };
  expect(first.changes).toHaveLength(1);
  await configureDirectoryAutomation(
    db,
    f.owner,
    {
      connectionId: f.connectionId,
      expectedVersion: 1,
      writeEnabled: false,
      mappings: [
        { attributeId: field.id, field: "firstname", direction: "read" },
      ],
    },
    key(),
  );
  await expect(
    mappedFields(
      db,
      f.owner,
      { ...input, previewToken: first.previewToken },
      key(),
      false,
    ),
  ).rejects.toMatchObject({ code: "MAPPING_PREVIEW_CHANGED" });
  const current = (await mappedFields(db, f.owner, input, key(), true)) as {
    previewToken: string;
  };
  await mappedFields(
    db,
    f.owner,
    { ...input, previewToken: current.previewToken },
    key(),
    false,
  );
  expect(
    (await readManagement(db, f.owner)).values.find(
      (v) => v.objectId === person,
    )?.values[field.id],
  ).toBe("Test");
  expect(provider.apply).not.toHaveBeenCalled();
});

it("allows one IT operator to directly apply a fresh preview while preserving legacy reviews",async()=>{
  const f=await fixture();await enable(f);
  const p=await previewDirectoryChange(db,f.it,f.input);
  const keyValue=key(),job=await submitDirectoryChange(db,f.it,f.input,p.token,keyValue,true);
  expect(job.reviewId).toBeNull();expect(job.status).toBe('queued');
  expect(await submitDirectoryChange(db,f.it,f.input,p.token,keyValue,true)).toEqual(job);
  await drainDirectoryOperations(db);
  expect(provider.apply).toHaveBeenCalledOnce();
  expect((await directoryAutomationData(db,f.it)).jobs.find(j=>j.id===job.id)).toMatchObject({approval_mode:'direct',status:'completed'});
  await expect(submitDirectoryChange(db,f.hr,f.input,p.token,key(),true)).rejects.toMatchObject({status:403});
});

it("resumes only unfinished batch steps with the same operation IDs",async()=>{
  const f=await fixture();await enable(f);
  const input=directoryChangeInput.parse({...f.input,additionalActions:[{kind:'group.members',objectId:'group',memberIds:['user'],action:'add'}]});
  const p=await previewDirectoryChange(db,f.it,input),job=await submitDirectoryChange(db,f.it,input,p.token,key(),true);
  provider.apply.mockResolvedValueOnce({verified:true}).mockRejectedValueOnce(new Error('transport outage'));
  await drainDirectoryOperations(db);
  expect((await directoryAutomationData(db,f.it)).jobs.find(j=>j.id===job.id)?.steps).toHaveLength(1);
  await db.pool.query("UPDATE people_directory_operations SET next_attempt_at=0 WHERE id=$1",[job.id]);
  await drainDirectoryOperations(db);
  expect(provider.apply.mock.calls.map(args=>args[1])).toEqual([job.id+':0',job.id+':1',job.id+':1']);
});

it("moves groups and OUs separately from HR structure and rejects OU cycles",()=>{
  expect(()=>validateObjectTarget({kind:'object.move',objectId:'group',parentId:'ou'},inventory)).not.toThrow();
  const nested={...inventory,objects:[...inventory.objects,{...inventory.objects[0],id:'child-ou',parentId:'ou'}]};
  expect(()=>validateObjectTarget({kind:'object.move',objectId:'ou',parentId:'child-ou'},nested)).toThrow('descendant');
});

it("rejects group cycles formed across individually valid batch steps",async()=>{
 const f=await fixture();await enable(f);
 const second={...inventory.objects.find(o=>o.id==='group')!,id:'second-group',memberIds:[]};
 await db.pool.query("UPDATE people_directory_state SET inventory=$2 WHERE connection_id=$1",[f.input.connectionId,JSON.stringify({...inventory,objects:[...inventory.objects,second]})]);
 const input=directoryChangeInput.parse({...f.input,action:{kind:'group.members',objectId:'group',memberIds:['second-group'],action:'add'},additionalActions:[{kind:'group.members',objectId:'second-group',memberIds:['group'],action:'add'}]});
 await expect(previewDirectoryChange(db,f.it,input)).rejects.toMatchObject({code:'DIRECTORY_GROUP_CYCLE'});
});

it("rechecks organization membership ownership after a manual removal was reviewed",async()=>{
  const f=await fixture();await enable(f);
  const input=directoryChangeInput.parse({...f.input,action:{kind:"group.members",objectId:"group",memberIds:["user"],action:"remove"}});
  const preview=await previewDirectoryChange(db,f.it,input);
  const job=await submitDirectoryChange(db,f.it,input,preview.token,key(),true);
  await db.pool.query("INSERT INTO people_directory_membership_sources(connection_id,group_id,member_id,source_id,created_at) VALUES($1,'group','user','gpo-rule:new',$2)",[f.connectionId,Date.now()]);
  await db.pool.query(`INSERT INTO organization_gpo_membership_claims(connection_id,group_id,member_id,source_id,rule_id,company_id,position_id,engagement_id,person_id,revision,engagement_version,gpo_id,computer_target_ou_id)
    VALUES($1,'group','user','gpo-rule:new','new-rule',$2,'position','engagement','person',1,1,'gpo','ou')`,[f.connectionId,f.company]);
  await drainDirectoryOperations(db);
  expect(provider.apply).not.toHaveBeenCalled();
  expect((await db.pool.query("SELECT status,last_error FROM people_directory_operations WHERE id=$1",[job.id])).rows[0])
    .toMatchObject({status:"blocked",last_error:"DIRECTORY_MEMBERSHIP_SOURCE_RETAINED"});
  // An explicit manual removal of an untracked member retains its existing meaning.
  await db.pool.query("DELETE FROM people_directory_membership_sources WHERE connection_id=$1",[f.connectionId]);
  const nextPreview=await previewDirectoryChange(db,f.it,input);
  await submitDirectoryChange(db,f.it,input,nextPreview.token,key(),true);
  await drainDirectoryOperations(db);
  expect(provider.apply).toHaveBeenCalledOnce();
});

it("retains a new organization source when a previously reviewed package removal runs",async()=>{
  const f=await fixture();await enable(f);
  const input=directoryChangeInput.parse({...f.input,action:{kind:"group.members",objectId:"group",memberIds:["user"],action:"remove"}});
  const preview=await previewDirectoryChange(db,f.it,input);
  const job=await submitDirectoryChange(db,f.it,input,preview.token,key(),true);
  const engagement=(await db.pool.query("SELECT id,version,person_id FROM hr_engagements WHERE company_id=$1 ORDER BY id LIMIT 1",[f.company])).rows[0];
  const provision=key(),oldApplication=key(),packageId=key();
  await db.pool.query("INSERT INTO people_access_packages(company_id,id,version,published_revision,definition) VALUES($1,$2,1,1,'{}')",[f.company,packageId]);
  await db.pool.query("INSERT INTO people_package_revisions(company_id,package_id,revision,definition,created_by,created_at) VALUES($1,$2,1,'{}',$3,$4)",[f.company,packageId,f.it.userId,Date.now()]);
  await db.pool.query(`INSERT INTO people_applied_packages(id,company_id,person_id,engagement_id,package_id,package_revision,definition,source_kind,scope,authorizing_assignment_id,valid_from,valid_until,created_at)
    VALUES($1,$2,$3,$4,$5,1,'{}','position','{}',$6,$7,$8,$7)`,[oldApplication,f.company,engagement.person_id,engagement.id,packageId,f.it.assignmentId,Date.now(),Date.now()+3600000]);
  await db.pool.query("INSERT INTO people_directory_membership_sources(connection_id,group_id,member_id,source_id,application_id,created_at) VALUES($1,'group','user',$2,$2,$3)",[f.connectionId,oldApplication,Date.now()]);
  await db.pool.query(`INSERT INTO people_provision_operations(id,company_id,definition,evidence,status,created_by,assignment_id,session_id,created_at,updated_at)
    VALUES($1,$2,$3,$4,'queued',$5,$6,'fixture-session',$7,$7)`,[
      provision,f.company,JSON.stringify({revokeOnly:true}),JSON.stringify({plans:[{id:engagement.id,engagementVersion:engagement.version,oldIds:[oldApplication],definition:{connectionId:f.connectionId,directoryGroupIds:[]},directoryMembers:["user"]}]}),f.it.userId,f.it.assignmentId,Date.now(),
    ]);
  await db.pool.query("UPDATE people_directory_operations SET provision_id=$2 WHERE id=$1",[job.id,provision]);
  await db.pool.query("INSERT INTO people_directory_membership_sources(connection_id,group_id,member_id,source_id,created_at) VALUES($1,'group','user','gpo-rule:after-preview',$2)",[f.connectionId,Date.now()]);
  await drainDirectoryOperations(db);
  expect(provider.apply).not.toHaveBeenCalled();
  const result=(await db.pool.query("SELECT status,steps FROM people_directory_operations WHERE id=$1",[job.id])).rows[0];
  expect(result.status).toBe("completed");
  expect(result.steps[0].result.retainedMemberIds).toEqual(["user"]);
  expect((await db.pool.query("SELECT source_id FROM people_directory_membership_sources WHERE connection_id=$1",[f.connectionId])).rows)
    .toEqual([{source_id:"gpo-rule:after-preview"}]);
});

it("schedules package-source cleanup for a shared GPO membership and retires it before finalization",async()=>{
  const {previewProvisioning,applyProvisioning,drainProvisioning}=await import("../src/server/technology");
  const {provisioningInput}=await import("../src/lib/technology");
  const {packageDefinition}=await import("../src/lib/people-lifecycle");
  const liveInventory=structuredClone(inventory);
  liveInventory.objects.find(object=>object.id==="group")!.memberIds=["user"];
  provider.read.mockImplementation(async()=>structuredClone(liveInventory));
  const f=await fixture();await enable(f);
  const engagement=(await db.pool.query("SELECT e.id,e.person_id,p.user_id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE e.company_id=$1 AND p.user_id=$2",[f.company,f.claimant.userId])).rows[0];
  const packageId=key(),oldApplication=key(),identityId=key(),now=Date.now();
  const recipe=packageDefinition.parse({name:"Shared Directory access",account:"univention",connectionId:f.connectionId,grants:[],groupIds:[],directoryGroupIds:["group"],tasks:[]});
  await db.pool.query("INSERT INTO people_access_packages(company_id,id,version,published_revision,definition) VALUES($1,$2,1,1,$3)",[f.company,packageId,JSON.stringify(recipe)]);
  await db.pool.query("INSERT INTO people_package_revisions(company_id,package_id,revision,definition,created_by,created_at) VALUES($1,$2,1,$3,$4,$5)",[f.company,packageId,JSON.stringify(recipe),f.it.userId,now]);
  await db.pool.query(`INSERT INTO people_applied_packages(id,company_id,person_id,engagement_id,package_id,package_revision,definition,source_kind,scope,authorizing_assignment_id,valid_from,valid_until,created_at)
    VALUES($1,$2,$3,$4,$5,1,$6,'position','{}',$7,$8,$9,$8)`,[oldApplication,f.company,engagement.person_id,engagement.id,packageId,JSON.stringify(recipe),f.it.assignmentId,now,now+3600000]);
  await db.pool.query("INSERT INTO iam_identities(id,company_id,connection_id,external_id,username,name,enabled,observed_at,user_id,person_id) VALUES($1,$2,$3,'user','user','Linked employee',true,$4,$5,$6)",[identityId,f.company,f.connectionId,now,engagement.user_id,engagement.person_id]);
  await db.pool.query(`INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,connection_id,identity_id)
    VALUES($1,$2,$3,$4,'active','directory',$5,$6) ON CONFLICT(company_id,user_id) DO UPDATE SET person_id=excluded.person_id,status='active',login_policy='directory',connection_id=excluded.connection_id,identity_id=excluded.identity_id`,[f.company,engagement.user_id,key(),engagement.person_id,f.connectionId,identityId]);
  await db.pool.query("INSERT INTO people_directory_membership_sources(connection_id,group_id,member_id,source_id,application_id,created_at) VALUES($1,'group','user',$2,$2,$3),($1,'group','user','gpo-rule:shared',NULL,$3)",[f.connectionId,oldApplication,now]);
  await db.pool.query(`INSERT INTO organization_gpo_membership_claims(connection_id,group_id,member_id,source_id,rule_id,company_id,position_id,engagement_id,person_id,revision,engagement_version,gpo_id,computer_target_ou_id)
    VALUES($1,'group','user','gpo-rule:shared','shared-rule',$2,'position',$3,$4,1,1,'gpo','ou')`,[f.connectionId,f.company,engagement.id,engagement.person_id]);
  const input=provisioningInput.parse({engagementIds:[engagement.id],packageId,revision:1,revokeOnly:true,revokeApplicationIds:[oldApplication],validUntil:now+3600000,reason:"Retire only the selected package source"});
  const preview=await previewProvisioning(db,f.it,input);
  expect(preview.employees[0].retained).toContain("Directory membership from another or untracked source · group");
  const operation=await applyProvisioning(db,f.it,input,preview.token,key());
  const children=(await db.pool.query("SELECT id,definition FROM people_directory_operations WHERE provision_id=$1",[operation.id])).rows;
  expect(children).toHaveLength(1);
  expect(children[0].definition.action).toMatchObject({kind:"group.members",objectId:"group",memberIds:["user"],action:"remove"});
  await drainDirectoryOperations(db);
  expect(provider.apply).not.toHaveBeenCalled();
  expect(liveInventory.objects.find(object=>object.id==="group")!.memberIds).toContain("user");
  expect((await db.pool.query("SELECT status,steps FROM people_directory_operations WHERE id=$1",[children[0].id])).rows[0])
    .toMatchObject({status:"completed",steps:[{status:"completed",result:{retainedMemberIds:["user"]}}]});
  expect((await db.pool.query("SELECT source_id FROM people_directory_membership_sources WHERE connection_id=$1",[f.connectionId])).rows).toEqual([{source_id:"gpo-rule:shared"}]);
  expect((await db.pool.query("SELECT ended_at FROM people_applied_packages WHERE id=$1",[oldApplication])).rows[0].ended_at).toBeNull();
  await drainProvisioning(db);
  expect((await db.pool.query("SELECT status FROM people_provision_operations WHERE id=$1",[operation.id])).rows[0].status).toBe("completed");
  expect((await db.pool.query("SELECT ended_at FROM people_applied_packages WHERE id=$1",[oldApplication])).rows[0].ended_at).not.toBeNull();
  expect((await db.pool.query("SELECT source_id FROM people_directory_membership_sources WHERE connection_id=$1",[f.connectionId])).rows).toEqual([{source_id:"gpo-rule:shared"}]);
});
