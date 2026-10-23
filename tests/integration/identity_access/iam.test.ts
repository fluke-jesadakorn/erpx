import { beforeAll, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import { iamDb } from "@erpx/server-identity_access/public/iam-db";
import { accessComparison, accessDetail, accessEntries, peopleAccessData } from "@erpx/server-people_payroll/public/people-access";
import { emptyDirectoryHierarchy, validateDirectoryHierarchy } from "@erpx/feature-organization/lib/hierarchy";
import { loadIamAuthority, iamRole, iamRoles } from "@erpx/server-identity_access/public/iam-store";
import { iamCommand } from "@erpx/server-identity_access/public/iam-transactions";
import {
  iamOverview,
  iamAuditHistory,
  saveIamRole,
  roleInput,
  decideIamReview,
  grantIamAccess,
  grantInput,
  revokeIamAssignment,
  startAccessReviewCampaign,
  decideAccessReviewItem,
  cancelAccessReviewCampaign,
} from "@erpx/server-identity_access/public/iam-service";
import {connectionInput,saveConnection,proposeMapping,mappingInput,applyDirectoryReview} from "@erpx/server-identity_access/public/iam-directories";
import {connectionById} from '@erpx/server-identity_access/public/directory-lookup';
import {applyDirectorySnapshot} from '@erpx/server-identity_access/public/directory-synchronization';
import {
  prepareMigration,
  migrationInput,
  applyMigration,
} from "@erpx/server-identity_access/public/iam-migrations";
import { sealSecret, totp, verifyMfa } from "@erpx/server-identity_access/public/iam-mfa";
import { adGuid, escapeLdap, ldapEnabled } from "@erpx/server-identity_access/public/iam-providers";
import {
  permissionCatalog,
  hasPermission,
  evaluateAccess,
  builtInPermissions,
} from "@erpx/feature-identity_access/lib/permissions";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { executeCommand } from "@erpx/server-workflow/public/commands";
import { selectWorkRole } from "@erpx/server-work_projection/public/work-role-store";
import type { IamActor } from "@erpx/server-identity_access/public/iam-context";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {workspaceGrantsForAccount,workspaceCompaniesForAccount} from "@erpx/server-identity_access/public/authorization";
import { escalateAccessReviews } from "@erpx/server-identity_access/public/iam-access-review-alerts";
import {privateFileDenialAuthoritySourceLocks,privateFileDenialAuthorityWitness} from '@erpx/server-identity_access/public/private-file-denial-authority';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import { listStaffNotifications } from "@erpx/server-collaboration/public/staff-notifications";
import {
  activateEmergency,
  listEmergencyAccess,
  revokeEmergency,
} from "@erpx/server-identity_access/public/iam-recovery";
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  runtimeEnv: async () => env,
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  holdingBindingBySlug: async (slug: string) => ({
    id: "h001",
    slug,
    displayName: "IAM rehearsal",
  }),
  activeHoldingBindings: async () => [
    { id: "h001", slug: "rehearsal", displayName: "IAM rehearsal" },
  ],
}));
const db = iamDb(env.H001_DB),
  key = () => crypto.randomUUID(),
  now = () => Date.now();
it("rejects duplicate, dangling, cyclic and excessive directory topology", () => {
  const h = emptyDirectoryHierarchy();
  expect(() => validateDirectoryHierarchy(h, ["u", "u"], [])).toThrow(/Duplicate/);
  expect(() => validateDirectoryHierarchy({ ...h, containers: [{ externalId: "ou", parentId: "missing", name: "OU", kind: "ou" }] }, [], [])).toThrow(/parent/);
  expect(() => validateDirectoryHierarchy({ ...h, groupEdges: [{ parentId: "a", childId: "b" }, { parentId: "b", childId: "a" }] }, [], ["a","b"])).toThrow(/cycle/);
  expect(() => validateDirectoryHierarchy({ ...h, placements: [{ kind: "identity", externalId: "u", containerId: "absent" }] }, ["u"], [])).toThrow(/placement/);
});
it("People explorer isolates company/subtree data, includes accountless people, and compares one assignment per person", async () => {
  const f = await fixture(), other = await fixture(), branch = key(), department = key(), outside = key();
  for (const [id, kind, parent] of [[branch,"branch",f.companyId],[department,"department",branch],[outside,"branch",f.companyId]]) await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,?,?,?,?,?,?)").bind(id,kind,id,id,parent,f.companyId,kind === "department" ? branch : null).run();
  const people: string[] = [];
  for (let index = 0; index < 33; index++) {
    const id = key(); people.push(id);
    await db.prepare("INSERT INTO hr_people(id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES(?,?,?,'TH','active',?,?,?)").bind(id,f.companyId,`Accountless ${index}`,f.owner.userId,now(),now()).run();
    await db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'employee','salaried','active','2026-01-01','Engineer','THB',?,?,?)").bind(key(),id,f.companyId,index === 32 ? outside : branch,index === 32 ? null : department,f.owner.userId,now(),now()).run();
  }
  const full = await peopleAccessData(db,f.owner);
  expect(full.entries.some(e => e.account === "No ERPX account")).toBe(true);
  expect(full.entries.some(e => e.userId === other.owner.userId)).toBe(false);
  const first = accessEntries(full,new URLSearchParams({ node: branch }));
  expect(first.items).toHaveLength(30); expect(first.nextCursor).toBeTruthy();
  const second = accessEntries(full,new URLSearchParams({ node: branch, cursor: first.nextCursor! }));
  expect(second.items).toHaveLength(2); expect(second.items.some(e => first.items.some(p => p.id === e.id))).toBe(false);
  expect(() => accessEntries(full,new URLSearchParams({ node: outside, cursor: first.nextCursor! }))).toThrow(/Restart/);
  const limited = await peopleAccessData(db,{ ...f.owner, scopeKind: "department", scopeNodeId: department, branchId: branch, departmentId: department });
  expect(limited.fullDirectory).toBe(false); expect(limited.tree.map(n => n.id)).toEqual([department]);
  expect(limited.entries.filter(e => e.kind === "person")).toHaveLength(32);
  expect(() => accessDetail(limited,`person:${people[32]}`)).toThrow(/unavailable/);
  expect(() => accessComparison(limited,[f.owner.assignmentId])).toThrow(/unavailable/);
  expect(accessComparison(full,[f.owner.assignmentId,f.employee.assignmentId]).decisions).toHaveLength(2);
  const aliases = { ...full, assignments: full.assignments.map(a => ({...a,person_id:"one-human"})) };
  expect(() => accessComparison(aliases,[f.owner.assignmentId,f.employee.assignmentId])).toThrow(/one work assignment/);
  expect(() => accessComparison(full,[f.owner.assignmentId,f.owner.assignmentId])).toThrow(/one assignment/);
  expect(() => accessComparison(full,Array.from({length:5},key))).toThrow(/four/);
  await expect(peopleAccessData(db,f.employee)).rejects.toMatchObject({code:"PERMISSION_REQUIRED"});
});
it("stores topology only with a complete sync, backfills without reseed, and retains it after invalid refresh", async () => {
  const f = await fixture(), saved = await saveConnection(db,f.owner,connectionInput.parse({ expectedVersion:0,name:"Hierarchy fixture",provider:"univention",config:{mode:"ldaps",secretProfile:"test",authority:"corp.test",ldapUrl:"ldaps://corp.test",baseDn:"dc=corp,dc=test"} }),key());
  await db.prepare("UPDATE iam_connections SET status='active' WHERE id=?").bind(saved.id).run();
  const c = await connectionById(db,f.companyId,saved.id), snapshot = { complete:true as const,users:[{externalId:"u",username:"staff",name:"Staff",email:null,enabled:true,groupIds:["g"]}],groups:[{externalId:"g",name:"Staff group"}] };
  await applyDirectorySnapshot(db,c,snapshot);
  const read = () => db.prepare("SELECT hierarchy_json FROM iam_directory_hierarchies WHERE connection_id=?").bind(saved.id).first<{hierarchy_json:ReturnType<typeof emptyDirectoryHierarchy>}>();
  expect((await read())?.hierarchy_json).toEqual(emptyDirectoryHierarchy());
  const hierarchy = { ...emptyDirectoryHierarchy(),containers:[{externalId:"ou",name:"People",kind:"ou" as const,parentId:null}],placements:[{kind:"identity" as const,externalId:"u",containerId:"ou"}] };
  await applyDirectorySnapshot(db,c,{...snapshot,hierarchy});
  expect((await read())?.hierarchy_json).toEqual(hierarchy);
  await expect(applyDirectorySnapshot(db,c,{...snapshot,hierarchy,complete:false} as unknown as typeof snapshot)).rejects.toMatchObject({code:"DIRECTORY_INCOMPLETE"});
  await expect(applyDirectorySnapshot(db,c,{...snapshot,hierarchy:{...hierarchy,groupEdges:[{parentId:"g",childId:"g"}]}})).rejects.toThrow(/cycle/);
  expect((await read())?.hierarchy_json).toEqual(hierarchy);
  const full = await peopleAccessData(db,f.owner); expect(full.tree.some(n=>n.kind === "ou")).toBe(true); expect(full.entries.some(e=>e.kind === "identity")).toBe(true);
  const noDirectory = await peopleAccessData(db,{...f.owner,iam:{...f.owner.iam!,permissions:f.owner.iam!.permissions.filter(p=>p!=="iam.directory.manage")}});
  expect(noDirectory.tree.some(n=>n.kind === "connection")).toBe(false); expect(noDirectory.entries.some(e=>e.kind === "identity")).toBe(false);
  await db.prepare("UPDATE iam_identities SET user_id=?,observed_at=? WHERE connection_id=?").bind(f.employee.userId,now()-600000,saved.id).run();
  const linked = await peopleAccessData(db,{...f.owner,iam:{...f.owner.iam!,permissions:f.owner.iam!.permissions.filter(p=>p!=="iam.directory.manage")}});
  expect(linked.entries.find(e=>e.kind === "identity")?.sync).toBe("Stale evidence");
  await applyDirectorySnapshot(db,c,{complete:true,users:[],groups:[],hierarchy:emptyDirectoryHierarchy()});
  expect((await read())?.hierarchy_json.containers).toEqual([]);
  expect((await peopleAccessData(db,f.owner)).entries.find(e=>e.kind === "identity")?.status).toBe("Disabled");
});
it("activates, lists, and immediately revokes only the current emergency session with replay-safe audit receipts", async () => {
  const f = await fixture(),
    owner = f.owner,
    input = {
      holdingSlug: "rehearsal",
      companyId: f.companyId,
      reason: "Restore access while the normal administrator is unavailable.",
    },
    activateKey = key(),
    revokeKey = key();
  await db
    .prepare(
      "INSERT INTO iam_members(company_id,user_id,principal_id,login_policy) VALUES(?,?,?,'emergency')",
    )
    .bind(f.companyId, owner.userId, `recovery:${owner.userId}`)
    .run();
  await db
    .prepare("INSERT INTO iam_emergency_sessions VALUES(?,?,?,?,?)")
    .bind(
      `sibling-${key()}`,
      f.companyId,
      owner.userId,
      now() + 120_000,
      "Other browser session",
    )
    .run();
  await db
    .prepare("INSERT INTO iam_emergency_sessions VALUES(?,?,?,?,?)")
    .bind(
      owner.sessionId,
      f.companyId,
      owner.userId,
      now() - 1,
      "Expired activation",
    )
    .run();
  await db
    .prepare("INSERT INTO iam_emergency_sessions VALUES(?,?,?,?,?)")
    .bind(
      owner.sessionId,
      `expired-${key()}`,
      owner.userId,
      now() - 1,
      "Expired in another company",
    )
    .run();

  const [activated, replayed] = await Promise.all([
    activateEmergency(owner, input, activateKey),
    activateEmergency(owner, input, activateKey),
  ]);
  expect(replayed).toEqual(activated);
  expect(activated.expiresAt).toBeGreaterThan(now());
  expect(
    await loadIamAuthority(
      db,
      f.companyId,
      owner.userId,
      "holding_owner",
      owner.sessionId,
    ),
  ).toMatchObject({ emergencyUntil: activated.expiresAt });
  await expect(
    activateEmergency(
      owner,
      { ...input, reason: "A different authorized recovery reason." },
      activateKey,
    ),
  ).rejects.toMatchObject({ code: "IDEMPOTENCY_CONFLICT" });

  const listed = await listEmergencyAccess(owner);
  expect(listed.unavailableScopes).toEqual([]);
  expect(listed.access).toEqual([
    expect.objectContaining({
      companyId: f.companyId,
      holdingSlug: "rehearsal",
      reason: input.reason,
      expiresAt: activated.expiresAt,
    }),
  ]);

  const revokeInput = {
      ...input,
      reason: "The administrator has restored normal access.",
    };
  await expect(
    revokeEmergency(owner, revokeInput, activateKey),
  ).rejects.toMatchObject({ code: "IDEMPOTENCY_CONFLICT" });
  const [revoked, revokeReplay] = await Promise.all([
    revokeEmergency(owner, revokeInput, revokeKey),
    revokeEmergency(owner, revokeInput, revokeKey),
  ]);
  expect(revokeReplay).toEqual(revoked);
  expect(revoked.revoked).toBe(true);
  await expect(
    loadIamAuthority(
      db,
      f.companyId,
      owner.userId,
      "holding_owner",
      owner.sessionId,
    ),
  ).rejects.toMatchObject({ code: "EMERGENCY_ACCESS_REQUIRED" });
  expect(
    await db
      .prepare(
        "SELECT count(*) count FROM iam_emergency_sessions WHERE company_id=? AND user_id=?",
      )
      .bind(f.companyId, owner.userId)
      .first<{ count: number }>(),
  ).toMatchObject({ count: 1 });
  expect(
    (
      await db
        .prepare(
          "SELECT action FROM iam_audit WHERE company_id=? AND actor_id=? AND action IN ('emergency.activated','emergency.revoked') ORDER BY action",
        )
        .bind(f.companyId, owner.userId)
        .all<{ action: string }>()
    ).results.map((row) => row.action),
  ).toEqual(["emergency.activated", "emergency.revoked"]);
});
beforeAll(async () => {
  await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS);
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
async function fixture() {
  const companyId = key();
  await db
    .prepare(
      "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'IAM rehearsal',?)",
    )
    .bind(companyId, companyId, companyId)
    .run();
  async function user(role = "holding_owner") {
    const userId = key(),
      assignmentId = key(),
      sessionId = key(),
      until = now() + 200 * 86400000;
    await env.CONTROL_DB.prepare(
      'INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?)',
    )
      .bind(userId, "IAM " + role, userId + "@example.test", now(), now())
      .run();
    await env.CONTROL_DB.prepare(
      'INSERT INTO session(id,"expiresAt",token,"createdAt","updatedAt","userId") VALUES(?,?,?,?,?,?)',
    )
      .bind(
        sessionId,
        new Date(now() + 86400000).toISOString(),
        key(),
        new Date().toISOString(),
        new Date().toISOString(),
        userId,
      )
      .run();
    await env.CONTROL_DB.prepare(
      "INSERT INTO iam_mfa(user_id,secret,enabled) VALUES(?,?,true)",
    )
      .bind(userId, await sealSecret("JBSWY3DPEHPK3PXP"))
      .run();
    await env.CONTROL_DB.prepare("INSERT INTO iam_mfa_sessions VALUES(?,?,?)")
      .bind(sessionId, userId, now())
      .run();
    await db
      .prepare(
        "INSERT INTO assignments(id,user_id,role,company_id,scope_kind,modules_json,valid_from,valid_until) VALUES(?,?,?,?,'holding','[]',0,?)",
      )
      .bind(assignmentId, userId, role, companyId, until)
      .run();
    await selectWorkRole(db, {
      sessionId,
      userId,
      companyId,
      assignmentId,
      assignmentVersion: 1,
      revision: 0,
      expiresAt: now() + 86400000,
    });
    return {
      userId,
      assignmentId,
      sessionId,
      role,
      companyId,
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
    employee = await user("employee");
  return { companyId, owner, reviewer, employee, user };
}
it('private-file denial witness observes unchanged invalid account authority without authorizing it and detects native or captured-context changes',async()=>{
 const f=await fixture(),actor:WorkContext={...f.employee,name:'Denied account',email:'denied@example.test',holdingSlug:'rehearsal',holdingName:'IAM rehearsal',scope:{kind:'holding',holdingId:'h001',holdingSlug:'rehearsal',holdingName:'IAM rehearsal',companyId:f.companyId},assignmentScope:'holding',inherited:false,modules:[],assignments:[],workspaces:[]};
 const observe=()=>db.transaction(async tx=>{await tx.batch(privateFileDenialAuthoritySourceLocks(tx));await lockQueuedCommandAuthority(tx);return privateFileDenialAuthorityWitness(tx,actor);});
 const before=await observe();expect(before).toMatch(/^[a-f0-9]{64}$/);expect(await observe()).toBe(before);
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(now(),actor.assignmentId).run();
 const invalid=await observe();expect(invalid).not.toBe(before);expect(await observe()).toBe(invalid);
 await db.transaction(async tx=>{await tx.batch(privateFileDenialAuthoritySourceLocks(tx));await lockQueuedCommandAuthority(tx);expect(await privateFileDenialAuthorityWitness(tx,{...actor,authority:{...actor.authority!,revision:actor.authority!.revision+1}})).not.toBe(invalid);});
 expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(actor.assignmentId).first()).toEqual({revoked_at:expect.any(Number),version:2});
});
it("seeds protected built-ins with a code-owned permission catalog and denies unknown actions", async () => {
  const f = await fixture(),
    role = await iamRole(db, f.companyId, "finance_approver");
  expect(role?.builtIn).toBe(true);
  expect(role?.permissions.toSorted()).toEqual(
    builtInPermissions("finance_approver").toSorted(),
  );
  expect(
    hasPermission({ role: "custom_none", permissions: [] }, "finance.read"),
  ).toBe(false);
  expect(evaluateAccess(f.owner, "missing").result).toBe("unavailable");
  expect(
    evaluateAccess(f.owner, "finance.approve", { inspect: true }).result,
  ).toBe("conditional");
  expect(new Set(permissionCatalog.map((p) => p.id)).size).toBe(
    permissionCatalog.length,
  );
});
it("keeps local login policy and assignments intact when a connector is only saved", async () => {
  const f = await fixture();
  const saved = await saveConnection(
    db,
    f.owner,
    connectionInput.parse({
      expectedVersion: 0,
      name: "UCS laboratory",
      provider: "univention",
      config: {
        mode: "ldaps",
        secretProfile: "ucs",
        authority: "corp.test",
        ldapUrl: "ldaps://ucs.corp.test:636",
        baseDn: "dc=corp,dc=test",
      },
    }),
    key(),
  );
  expect(saved.status).toBe("draft");
  expect(
    (await db
      .prepare("SELECT count(*) n FROM iam_identities WHERE connection_id=?")
      .bind(saved.id)
      .first<{ n: number }>())!.n,
  ).toBe(0);
  expect(
    (await loadIamAuthority(db, f.companyId, f.employee.userId, "employee"))
      .identityVersion,
  ).toBe(0);
  expect((await iamOverview(db, f.owner)).connections).toHaveLength(1);
});
it("publishes only independently reviewed immutable role revisions and uses them in matrix decisions", async () => {
  const f = await fixture(),
    input = roleInput.parse({
      name: "Ledger viewer",
      expectedVersion: 0,
      permissions: ["finance.read", "iam.self"],
      scopes: ["holding"],
      reason: "Read-only finance work",
    });
  const proposal = await saveIamRole(db, f.owner, input, key());
  await expect(
    decideIamReview(
      db,
      f.owner,
      proposal.id,
      { expectedVersion: 1, decision: "approve", reason: "Self approval" },
      key(),
    ),
  ).rejects.toMatchObject({ code: "INDEPENDENT_REVIEW_REQUIRED" });
  await decideIamReview(
    db,
    f.reviewer,
    proposal.id,
    {
      expectedVersion: 1,
      decision: "approve",
      reason: "Reviewed scope and permissions",
    },
    key(),
  );
  const custom = await iamRole(db, f.companyId, proposal.roleId);
  expect(custom?.permissions).toEqual(input.permissions);
  await expect(
    db
      .prepare(
        "UPDATE iam_role_revisions SET permissions_json='[]' WHERE role_id=?",
      )
      .bind(proposal.roleId)
      .run(),
  ).rejects.toThrow("Immutable");
  const grant = await grantIamAccess(
    db,
    f.owner,
    grantInput.parse({
      userId: f.employee.userId,
      roleId: proposal.roleId,
      scopeKind: "holding",
      validUntil: now() + 86400000,
      reason: "Assigned finance reading",
    }),
    key(),
  );
  await decideIamReview(
    db,
    f.reviewer,
    grant.id,
    {
      expectedVersion: 1,
      decision: "approve",
      reason: "Independent grant review",
    },
    key(),
  );
  const matrix = await iamOverview(db, f.owner),
    assignment = matrix.assignments.find((a) => a.role === proposal.roleId)!;
  const decisions = matrix.decisions.find(
    (d) => d.assignmentId === assignment.id,
  )!.decisions;
  expect(decisions.find((d) => d.permission === "finance.read")!.result).toBe(
    "allowed",
  );
  expect(decisions.find((d) => d.permission === "finance.write")!.result).toBe(
    "denied",
  );
  const authority = await loadIamAuthority(
    db,
    f.companyId,
    f.employee.userId,
    proposal.roleId,
  );
  expect(
    hasPermission({ role: proposal.roleId, iam: authority }, "finance.read"),
  ).toBe(true);
  expect(
    hasPermission({ role: proposal.roleId, iam: authority }, "finance.write"),
  ).toBe(false);
});
it("offers Companies to the current published Legal, Support and Quality holding roles and immediately removes a revoked grant",async()=>{
 const f=await fixture(),other=await fixture(),roles=["legal_manager","legal_counsel","legal_requester","support_agent","support_manager","quality_inspector","quality_manager"],people=[];
 for(const role of roles){const person=await f.user(role);people.push(person);expect(await workspaceCompaniesForAccount(db,person.userId)).toEqual([{id:f.companyId,name:"IAM rehearsal"}]);}
 const person=people[0];await revokeIamAssignment(db,f.owner,person.assignmentId,1,"Withdraw only this exact Legal holding grant",key());
 expect(await workspaceCompaniesForAccount(db,person.userId)).toEqual([]);
 expect(await workspaceCompaniesForAccount(db,people[1].userId)).toEqual([{id:f.companyId,name:"IAM rehearsal"}]);
 expect((await workspaceCompaniesForAccount(db,f.owner.userId)).map(row=>row.id)).not.toContain(other.companyId);
 expect(await workspaceCompaniesForAccount(db,"unassigned-account")).toEqual([]);
});
it("uses independently published custom-role scopes for Company choices without extending the existing assignment scope",async()=>{
 const f=await fixture(),permissions=["records.read","iam.self"],reason="Independently reviewed native Company chooser scope",first=await saveIamRole(db,f.owner,roleInput.parse({name:"Scoped Company reader",expectedVersion:0,permissions,scopes:["holding"],reason}),key());
 await decideIamReview(db,f.reviewer,first.id,{expectedVersion:1,decision:"approve",reason},key());
 const proposed=await grantIamAccess(db,f.owner,grantInput.parse({userId:f.employee.userId,roleId:first.roleId,scopeKind:"holding",validUntil:now()+86400000,reason}),key());
 await decideIamReview(db,f.reviewer,proposed.id,{expectedVersion:1,decision:"approve",reason},key());
 await revokeIamAssignment(db,f.owner,f.employee.assignmentId,1,"Keep only the independently reviewed custom grant",key());
 const grant=(await db.prepare("SELECT id,scope_kind,scope_node_id,version FROM assignments WHERE company_id=? AND user_id=? AND role=? AND revoked_at IS NULL").bind(f.companyId,f.employee.userId,first.roleId).first<{id:string;scope_kind:string;scope_node_id:string|null;version:number}>())!;
 expect(await workspaceCompaniesForAccount(db,f.employee.userId)).toEqual([{id:f.companyId,name:"IAM rehearsal"}]);
 const narrowed=await saveIamRole(db,f.owner,roleInput.parse({id:first.roleId,name:"Scoped Company reader",expectedVersion:1,permissions,scopes:["branch"],reason:"Publish the current branch-only role scope without moving a grant"}),key());
 await decideIamReview(db,f.reviewer,narrowed.id,{expectedVersion:1,decision:"approve",reason},key());
 expect(await workspaceCompaniesForAccount(db,f.employee.userId)).toEqual([]);
 expect(await db.prepare("SELECT id,scope_kind,scope_node_id,version FROM assignments WHERE id=?").bind(grant.id).first()).toEqual({...grant,version:grant.version+1});
 expect((await iamRole(db,f.companyId,first.roleId))?.scopes).toEqual(["branch"]);
});
it("enforces MFA freshness, grant ceilings, parent expiry, and revoked proposer authority", async () => {
  const f = await fixture(),
    viewer = await f.user("auditor");
  await expect(
    grantIamAccess(
      db,
      viewer,
      grantInput.parse({
        userId: f.employee.userId,
        roleId: "holding_owner",
        scopeKind: "holding",
        reason: "Exceed authority",
      }),
      key(),
    ),
  ).rejects.toMatchObject({ status: 403 });
  await expect(
    grantIamAccess(
      db,
      f.owner,
      grantInput.parse({
        userId: f.employee.userId,
        roleId: "employee",
        scopeKind: "holding",
        validUntil: now() + 300 * 86400000,
        reason: "Too much time",
      }),
      key(),
    ),
  ).rejects.toMatchObject({ code: "EXPIRY_INVALID" });
  await env.CONTROL_DB.prepare(
    "UPDATE iam_mfa_sessions SET verified_at=0 WHERE session_id=?",
  )
    .bind(f.owner.sessionId)
    .run();
  await expect(
    saveIamRole(
      db,
      f.owner,
      roleInput.parse({
        name: "No fresh MFA",
        expectedVersion: 0,
        permissions: [],
        scopes: ["holding"],
        reason: "Check fresh MFA",
      }),
      key(),
    ),
  ).rejects.toMatchObject({ code: "FRESH_MFA_REQUIRED" });
  const p = await saveIamRole(
    db,
    f.reviewer,
    roleInput.parse({
      name: "Revoked proposal",
      expectedVersion: 0,
      permissions: [],
      scopes: ["holding"],
      reason: "Review will outlive proposer",
    }),
    key(),
  );
  await db
    .prepare("UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?")
    .bind(now(), f.reviewer.assignmentId)
    .run();
  const other = await f.user();
  await expect(
    decideIamReview(
      db,
      other,
      p.id,
      {
        expectedVersion: 1,
        decision: "approve",
        reason: "Cannot approve revoked authority",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "PROPOSER_AUTHORITY_REVOKED" });
});
it("invalidates child assignments and command replay immediately after parent revocation", async () => {
  const f = await fixture(),
    child = key();
  await db
    .prepare(
      "INSERT INTO assignments(id,user_id,role,company_id,scope_kind,modules_json,valid_from,valid_until,parent_id,source) VALUES(?,?,'employee',?,'holding','[]',0,?,?,'manual')",
    )
    .bind(
      child,
      f.employee.userId,
      f.companyId,
      now() + 86400000,
      f.owner.assignmentId,
    )
    .run();
  await db
    .prepare("UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?")
    .bind(now(), f.owner.assignmentId)
    .run();
  expect(
    await db
      .prepare("SELECT iam_assignment_active(?,?) active")
      .bind(child, now())
      .first(),
  ).toEqual({ active: false });
  const command = key();
  await expect(
    executeCommand(
      db,
      { ...f.employee, assignmentId: child, authority: undefined },
      command,
      "iam.test.revoked",
      {},
      command,
      1,
      {},
      [],
    ),
  ).rejects.toThrow();
});
it("rejects stale identity snapshots and stale work contexts before recording mutations", async () => {
  const f = await fixture();
  await db
    .prepare(
      "INSERT INTO iam_members(company_id,user_id,principal_id) VALUES(?,?,?)",
    )
    .bind(f.companyId, f.owner.userId, "local:" + f.owner.userId)
    .run();
  await expect(
    iamCommand(db, f.owner, key(), "test", {}, "iam.manage", async () => ({
      done: true,
    })),
  ).rejects.toMatchObject({ code: "IDENTITY_CHANGED" });
  const k = key();
  await expect(
    executeCommand(db, f.owner, k, "iam.test.stale", {}, k, 1, {}, []),
  ).rejects.toThrow();
  expect(
    await db
      .prepare("SELECT 1 FROM command_receipts WHERE command_id=?")
      .bind(k)
      .first(),
  ).toBeNull();
});
it("keeps distinct provider identities despite duplicate contact emails, and rejects partial authoritative imports", async () => {
  const f = await fixture(),
    connections = [];
  for (const provider of ["active_directory", "univention"] as const) {
    const saved = await saveConnection(
      db,
      f.owner,
      connectionInput.parse({
        expectedVersion: 0,
        name: provider,
        provider,
        config: {
          mode: "ldaps",
          secretProfile: provider,
          authority: "same.corp",
          ldapUrl: "ldaps://dc.same.corp",
          baseDn: "dc=same,dc=corp",
        },
      }),
      key(),
    );
    await db
      .prepare(
        "UPDATE iam_connections SET status='active',tested_version=version WHERE id=?",
      )
      .bind(saved.id)
      .run();
    connections.push(await connectionById(db, f.companyId, saved.id));
  }
  const snapshot = {
    complete: true as const,
    groups: [{ externalId: "g1", name: "Group" }],
    users: [
      {
        externalId: "immutable-1",
        username: "same",
        name: "Directory user",
        email: f.employee.userId + "@example.test",
        enabled: true,
        groupIds: ["g1"],
      },
    ],
  };
  for (const c of connections) await applyDirectorySnapshot(db, c, snapshot);
  expect(
    (
      await db
        .prepare("SELECT id,user_id FROM iam_identities WHERE company_id=?")
        .bind(f.companyId)
        .all()
    ).results,
  ).toHaveLength(2);
  await expect(
    applyDirectorySnapshot(db, connections[0], {
      ...snapshot,
      complete: false,
    } as unknown as typeof snapshot),
  ).rejects.toMatchObject({ code: "DIRECTORY_INCOMPLETE" });
  expect(
    (
      await db
        .prepare("SELECT enabled FROM iam_identities WHERE connection_id=?")
        .bind(connections[0].id)
        .first()
    )?.enabled,
  ).toBe(true);
  await applyDirectorySnapshot(db, connections[0], {
    complete: true,
    users: [],
    groups: [],
  });
  expect(
    (
      await db
        .prepare("SELECT enabled FROM iam_identities WHERE connection_id=?")
        .bind(connections[0].id)
        .first()
    )?.enabled,
  ).toBe(false);
});
it("rejects cross-company access and person aliases cannot approve their own proposal", async () => {
  const f = await fixture(),
    other = await fixture();
  await expect(
    revokeIamAssignment(
      db,
      other.owner,
      f.employee.assignmentId,
      1,
      "Cross-company",
      key(),
    ),
  ).rejects.toMatchObject({ code: "ASSIGNMENT_NOT_FOUND" });
  const person = key();
  for (const a of [f.owner, f.reviewer])
    await db
      .prepare(
        "INSERT INTO iam_members(company_id,user_id,principal_id,person_id) VALUES(?,?,?,?)",
      )
      .bind(f.companyId, a.userId, key(), person)
      .run();
  f.owner.iam = await loadIamAuthority(
    db,
    f.companyId,
    f.owner.userId,
    f.owner.role,
  );
  f.reviewer.iam = await loadIamAuthority(
    db,
    f.companyId,
    f.reviewer.userId,
    f.reviewer.role,
  );
  expect(samePerson(f.reviewer, f.owner.userId)).toBe(true);
  const p = await saveIamRole(
    db,
    f.owner,
    roleInput.parse({
      name: "Alias review",
      expectedVersion: 0,
      permissions: [],
      scopes: ["holding"],
      reason: "Identity separation check",
    }),
    key(),
  );
  await expect(
    decideIamReview(
      db,
      f.reviewer,
      p.id,
      {
        expectedVersion: 1,
        decision: "approve",
        reason: "Same person other login",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "INDEPENDENT_REVIEW_REQUIRED" });
});
it("uses correct AD GUID byte order without mutating input, escapes filters, and respects disablement", () => {
  const bytes = Buffer.from("33221100554477668899aabbccddeeff", "hex"),
    copy = Buffer.from(bytes);
  expect(adGuid(bytes)).toBe("00112233-4455-6677-8899-aabbccddeeff");
  expect(bytes).toEqual(copy);
  expect(escapeLdap("a*)(uid=*)")).toBe("a\\2a\\29\\28uid=\\2a\\29");
  expect(ldapEnabled({ dn: "x", userAccountControl: "514" }, true)).toBe(false);
  expect(ldapEnabled({ dn: "x", sambaAcctFlags: "[UD]" }, false)).toBe(false);
  expect(
    ldapEnabled({ dn: "x", pwdAccountLockedTime: "000001010000Z" }, false),
  ).toBe(false);
});
it("prevents authenticator replay and rejects MFA proof after the five minute boundary", async () => {
  const f = await fixture(),
    counter = Math.floor(now() / 30000),
    code = totp("JBSWY3DPEHPK3PXP", counter);
  await verifyMfa(f.owner, code);
  await expect(verifyMfa(f.owner, code)).rejects.toMatchObject({
    code: "MFA_INVALID",
  });
});
it("migrates an explicit batch, preserves history, and never restores local access after group removal", async () => {
  const f = await fixture(),
    recovery = await f.user(),
    outside = await f.user("employee"),
    personId = key();
  await env.CONTROL_DB.prepare(
    'INSERT INTO account(id,"accountId","providerId","userId",password,"createdAt","updatedAt") VALUES(?,?,\'credential\',?,?,?,?)',
  )
    .bind(key(), recovery.userId, recovery.userId, "fixture-hash", now(), now())
    .run();
  await db
    .prepare(
      "INSERT INTO iam_members(company_id,user_id,principal_id,login_policy) VALUES(?,?,?,'emergency')",
    )
    .bind(f.companyId, recovery.userId, "local:" + recovery.userId)
    .run();
  await db
    .prepare(
      "INSERT INTO hr_people(id,company_id,legal_name,country_code,status,user_id,created_by,created_at,updated_at) VALUES(?,?,?,'TH','active',?,?,?,?)",
    )
    .bind(
      personId,
      f.companyId,
      "Migrating employee",
      f.employee.userId,
      f.owner.userId,
      now(),
      now(),
    )
    .run();
  await db
    .prepare(
      "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES(?,?,?,?,?,'test',?,1,'History before migration',?)",
    )
    .bind(
      key(),
      key(),
      f.employee.userId,
      f.employee.assignmentId,
      "test.history",
      personId,
      now(),
    )
    .run();
  const saved = await saveConnection(
    db,
    f.owner,
    connectionInput.parse({
      expectedVersion: 0,
      name: "AD migration",
      provider: "active_directory",
      config: {
        mode: "ldaps",
        secretProfile: "ad",
        authority: "corp.test",
        ldapUrl: "ldaps://dc.corp.test",
        baseDn: "dc=corp,dc=test",
      },
    }),
    key(),
  );
  await db
    .prepare(
      "UPDATE iam_connections SET status='active',tested_version=version WHERE id=?",
    )
    .bind(saved.id)
    .run();
  const connection = await connectionById(db, f.companyId, saved.id);
  const snapshot = {
    complete: true as const,
    groups: [{ externalId: "g", name: "Pilots" }],
    users: [
      {
        externalId: key(),
        username: "pilot",
        name: "Migrating employee",
        email: "same@example.test",
        enabled: true,
        groupIds: ["g"],
      },
    ],
  };
  await applyDirectorySnapshot(db, connection, snapshot);
  const identity = await db
    .prepare("SELECT id FROM iam_identities WHERE connection_id=?")
    .bind(saved.id)
    .first<{ id: string }>();
  const input = migrationInput.parse({
    connectionId: saved.id,
    users: [
      {
        localUserId: f.employee.userId,
        personId,
        identityId: identity!.id,
        grants: [
          {
            roleId: "employee",
            scopeKind: "holding",
            validUntil: now() + 86400000,
            reason: "Explicit employee access",
          },
        ],
      },
    ],
    reason: "Selected pilot migration",
  });
  const p = await prepareMigration(db, f.owner, input, key());
  expect(
    (
      await db
        .prepare("SELECT revoked_at FROM assignments WHERE id=?")
        .bind(f.employee.assignmentId)
        .first()
    )?.revoked_at,
  ).toBeNull();
  await decideIamReview(
    db,
    f.reviewer,
    p.id,
    {
      expectedVersion: 1,
      decision: "approve",
      reason: "Verified employee and replacement grants",
    },
    key(),
    (c, _kind, payload) => applyMigration(c, f.reviewer, payload),
  );
  const principal = await db
    .prepare("SELECT user_id FROM iam_identities WHERE id=?")
    .bind(identity!.id)
    .first<{ user_id: string }>();
  expect(principal!.user_id).not.toBe(f.employee.userId);
  expect(
    (
      await db
        .prepare(
          "SELECT status FROM iam_members WHERE user_id=? AND company_id=?",
        )
        .bind(f.employee.userId, f.companyId)
        .first()
    )?.status,
  ).toBe("superseded");
  await expect(
    loadIamAuthority(db, f.companyId, f.employee.userId, "employee"),
  ).rejects.toMatchObject({ code: "IDENTITY_SUSPENDED" });
  expect(
    (await loadIamAuthority(db, f.companyId, outside.userId, "employee"))
      .identityVersion,
  ).toBe(0);
  const fresh = await loadIamAuthority(
    db,
    f.companyId,
    principal!.user_id,
    "employee",
  );
  expect(
    samePerson({ userId: principal!.user_id, iam: fresh }, f.employee.userId),
  ).toBe(true);
  expect(
    (
      await db
        .prepare(
          "SELECT actor_id FROM audit_events WHERE action='test.history' AND record_id=?",
        )
        .bind(personId)
        .first()
    )?.actor_id,
  ).toBe(f.employee.userId);
  const mappingReview = await proposeMapping(
    db,
    f.owner,
    mappingInput.parse({
      connectionId: saved.id,
      groupId: "g",
      roleId: "it_admin",
      scopeKind: "holding",
      validUntil: now() + 86400000,
      reason: "Approved pilot IT mapping",
    }),
    key(),
  );
  const reviewerIdentity = key();
  await db
    .prepare(
      "INSERT INTO iam_identities(id,company_id,connection_id,external_id,username,name,enabled,groups_json,observed_at,user_id) VALUES(?,?,?,?,?,? ,true,'[\"g\"]',?,?)",
    )
    .bind(
      reviewerIdentity,
      f.companyId,
      saved.id,
      key(),
      "reviewer",
      "Mapping beneficiary",
      now(),
      f.reviewer.userId,
    )
    .run();
  await expect(
    decideIamReview(
      db,
      f.reviewer,
      mappingReview.id,
      {
        expectedVersion: 1,
        decision: "approve",
        reason: "Attempt to approve own group grant",
      },
      key(),
      (c, kind, p) => applyDirectoryReview(c, f.reviewer, kind, p),
    ),
  ).rejects.toMatchObject({ code: "INDEPENDENT_REVIEW_REQUIRED" });
  await db
    .prepare("DELETE FROM iam_identities WHERE id=?")
    .bind(reviewerIdentity)
    .run();
  await decideIamReview(
    db,
    f.reviewer,
    mappingReview.id,
    {
      expectedVersion: 1,
      decision: "approve",
      reason: "Verified group and bounded IT grant",
    },
    key(),
    (c, kind, p) => applyDirectoryReview(c, f.reviewer, kind, p),
  );
  await applyDirectorySnapshot(db, connection, snapshot);
  const mapped = await db
    .prepare(
      "SELECT id FROM assignments WHERE user_id=? AND company_id=? AND source='directory' AND revoked_at IS NULL",
    )
    .bind(principal!.user_id, f.companyId)
    .first<{ id: string }>();
  expect(mapped).not.toBeNull();
  const matrix = await iamOverview(db, f.owner);
  expect(
    matrix.decisions.find((d) => d.assignmentId === mapped!.id)!.decisions[0]
      .source,
  ).toContain("Pilots");
  await applyDirectorySnapshot(db, connection, {
    ...snapshot,
    users: [
      { ...snapshot.users[0], groupIds: [] },
      {
        externalId: key(),
        username: "new",
        name: "Later group member",
        email: null,
        enabled: true,
        groupIds: ["g"],
      },
    ],
  });
  expect(
    (await db
      .prepare("SELECT revoked_at FROM assignments WHERE id=?")
      .bind(mapped!.id)
      .first())!.revoked_at,
  ).not.toBeNull();
  expect(
    (await db
      .prepare(
        "SELECT count(*) n FROM assignments WHERE user_id=? AND company_id=? AND source='manual' AND revoked_at IS NULL",
      )
      .bind(principal!.user_id, f.companyId)
      .first<{ n: number }>())!.n,
  ).toBe(1);
  expect(
    (
      await db
        .prepare(
          "SELECT count(*) n FROM iam_members WHERE company_id=? AND login_policy='directory'",
        )
        .bind(f.companyId)
        .first()
    )?.n,
  ).toBe(1);
  expect(
    (
      await db
        .prepare(
          "SELECT status FROM iam_members WHERE user_id=? AND company_id=?",
        )
        .bind(f.employee.userId, f.companyId)
        .first()
    )?.status,
  ).toBe("superseded");
  await db
    .prepare("UPDATE iam_connections SET status='disabled' WHERE id=?")
    .bind(connection.id)
    .run();
  await expect(
    loadIamAuthority(db, f.companyId, principal!.user_id, "employee"),
  ).rejects.toMatchObject({ code: "DIRECTORY_ACCESS_UNAVAILABLE" });
});
it("expires directory evidence at five minutes and never turns an outage into local permission", async () => {
  const f = await fixture(),
    saved = await saveConnection(
      db,
      f.owner,
      connectionInput.parse({
        expectedVersion: 0,
        name: "UCS evidence",
        provider: "univention",
        config: {
          mode: "ldaps",
          secretProfile: "ucs",
          authority: "ucs.corp",
          ldapUrl: "ldaps://ucs.corp",
          baseDn: "dc=corp",
        },
      }),
      key(),
    );
  await db
    .prepare("UPDATE iam_connections SET status='active' WHERE id=?")
    .bind(saved.id)
    .run();
  const connection = await connectionById(db, f.companyId, saved.id),
    externalId = key();
  await applyDirectorySnapshot(db, connection, {
    complete: true,
    groups: [],
    users: [
      {
        externalId,
        username: "staff",
        name: "Staff",
        email: null,
        enabled: true,
        groupIds: [],
      },
    ],
  });
  const identity = await db
    .prepare("SELECT id FROM iam_identities WHERE connection_id=?")
    .bind(saved.id)
    .first<{ id: string }>();
  await db
    .prepare("UPDATE iam_identities SET user_id=?,observed_at=? WHERE id=?")
    .bind(f.employee.userId, now() - 300001, identity!.id)
    .run();
  await db
    .prepare(
      "INSERT INTO iam_members(company_id,user_id,principal_id,login_policy,connection_id,identity_id) VALUES(?,?,?,'directory',?,?)",
    )
    .bind(f.companyId, f.employee.userId, key(), saved.id, identity!.id)
    .run();
  await expect(
    loadIamAuthority(db, f.companyId, f.employee.userId, "employee"),
  ).rejects.toMatchObject({ code: "DIRECTORY_ACCESS_UNAVAILABLE" });
  expect(
    (await iamOverview(db, f.owner)).assignments.find(
      (a) => a.id === f.employee.assignmentId,
    )?.active,
  ).toBe(false);
});

it("keeps emergency recovery independent from delegated company ownership", async () => {
  const f = await fixture(),
    recovery = await f.user();
  await expect(db
    .prepare("UPDATE assignments SET parent_id=?,valid_until=? WHERE id=?")
    .bind(f.owner.assignmentId, now() + 86400000, recovery.assignmentId)
    .run()).rejects.toThrow(/Delegated permission ceiling exceeded/);
  await db
    .prepare(
      "INSERT INTO iam_members(company_id,user_id,principal_id,login_policy) VALUES(?,?,?,'emergency')",
    )
    .bind(f.companyId, recovery.userId, "local:" + recovery.userId)
    .run();
  await db
    .prepare("UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?")
    .bind(now(), f.owner.assignmentId)
    .run();
  expect((await db.prepare("SELECT parent_id FROM assignments WHERE id=?")
    .bind(recovery.assignmentId).first<{parent_id:string|null}>())?.parent_id).toBeNull();
  expect((await db.prepare("SELECT iam_assignment_active(?,?) ok")
    .bind(f.owner.assignmentId, now()).first())!.ok).toBe(false);
  expect(
    (await db
      .prepare("SELECT iam_assignment_active(?,?) ok")
      .bind(recovery.assignmentId, now())
      .first())!.ok,
  ).toBe(true);
});
it("completes an independent access review and revokes access immediately",async()=>{
  const f=await fixture(),third=await f.user(),campaignKey=key();
  const campaign=await startAccessReviewCampaign(db,f.owner,{name:"Quarterly access certification",dueAt:now()+7*86400000,reason:"Confirm every active company assignment"},campaignKey);
  const items=(await db.prepare("SELECT * FROM iam_access_review_items WHERE campaign_id=? ORDER BY user_id").bind(campaign.id).all<{id:string;user_id:string;assignment_id:string;decision:string}>()).results;
  expect(items).toHaveLength(4);
  const employee=items.find(i=>i.user_id===f.employee.userId)!,owner=items.find(i=>i.user_id===f.owner.userId)!,reviewer=items.find(i=>i.user_id===f.reviewer.userId)!,thirdItem=items.find(i=>i.user_id===third.userId)!;
  await expect(decideAccessReviewItem(db,f.owner,employee.id,{expectedCampaignVersion:1,decision:"keep",reason:"Self-created campaign"},key())).rejects.toMatchObject({code:"INDEPENDENT_REVIEW_REQUIRED"});
  await expect(decideAccessReviewItem(db,f.reviewer,reviewer.id,{expectedCampaignVersion:1,decision:"keep",reason:"Own assignment"},key())).rejects.toMatchObject({code:"SELF_ACCESS_REVIEW"});
  const revokeKey=key();
  await decideAccessReviewItem(db,f.reviewer,employee.id,{expectedCampaignVersion:1,decision:"revoke",reason:"Employment access is no longer required"},revokeKey);
  await decideAccessReviewItem(db,f.reviewer,employee.id,{expectedCampaignVersion:1,decision:"revoke",reason:"Employment access is no longer required"},revokeKey);
  expect((await db.prepare("SELECT revoked_at FROM assignments WHERE id=?").bind(f.employee.assignmentId).first<{revoked_at:number|null}>())?.revoked_at).toBeTypeOf("number");
  expect((await db.prepare("SELECT assignment_id FROM active_work_contexts WHERE session_id=?").bind(f.employee.sessionId).first<{assignment_id:string|null}>())?.assignment_id).toBeNull();
  await decideAccessReviewItem(db,f.reviewer,owner.id,{expectedCampaignVersion:2,decision:"keep",reason:"Company ownership remains required"},key());
  await decideAccessReviewItem(db,f.reviewer,thirdItem.id,{expectedCampaignVersion:3,decision:"keep",reason:"Independent access reviewer remains required"},key());
  const finished=await decideAccessReviewItem(db,third,reviewer.id,{expectedCampaignVersion:4,decision:"keep",reason:"Reviewed administrator remains required"},key());
  expect(finished.campaignStatus).toBe("completed");
  expect(await db.prepare("SELECT status,version FROM iam_access_review_campaigns WHERE id=?").bind(campaign.id).first()).toEqual({status:"completed",version:5});
  expect((await db.prepare("SELECT action FROM iam_access_review_events WHERE campaign_id=? ORDER BY created_at,id").bind(campaign.id).all<{action:string}>()).results.map(e=>e.action)).toEqual(expect.arrayContaining(["started","revoked","kept","completed"]));
  await expect(db.prepare("UPDATE iam_access_review_events SET action='cancelled' WHERE campaign_id=?").bind(campaign.id).run()).rejects.toThrow("IMMUTABLE_IAM_ACCESS_REVIEW_EVENT");
});
it("rejects stale evidence and cancels only untouched access reviews",async()=>{
  const f=await fixture(),campaign=await startAccessReviewCampaign(db,f.owner,{name:"Access evidence check",dueAt:now()+86400000,reason:"Check current assignment evidence"},key()),items=(await db.prepare("SELECT id,assignment_id,user_id FROM iam_access_review_items WHERE campaign_id=?").bind(campaign.id).all<{id:string;assignment_id:string;user_id:string}>()).results;
  await db.prepare("UPDATE assignments SET version=version+1 WHERE id=?").bind(f.employee.assignmentId).run();
  await expect(decideAccessReviewItem(db,f.reviewer,items.find(i=>i.user_id===f.employee.userId)!.id,{expectedCampaignVersion:1,decision:"keep",reason:"Evidence changed"},key())).rejects.toMatchObject({code:"REVIEW_SOURCE_CHANGED"});
  await expect(cancelAccessReviewCampaign(db,f.reviewer,campaign.id,1,"Wrong owner",key())).rejects.toMatchObject({code:"CAMPAIGN_OWNER_REQUIRED"});
  await cancelAccessReviewCampaign(db,f.owner,campaign.id,1,"Restart with current assignment evidence",key());
  expect(await db.prepare("SELECT status,version FROM iam_access_review_campaigns WHERE id=?").bind(campaign.id).first()).toEqual({status:"cancelled",version:2});
});
it("reminds current access reviewers once before due and escalates once after overdue",async()=>{
  const f=await fixture(),clock=now(),campaign=await startAccessReviewCampaign(db,f.owner,{name:"Access review reminder",dueAt:clock+6*86400000,reason:"Certify access before the quarterly deadline"},key());
  expect(await escalateAccessReviews(db,clock)).toEqual({dueSoon:2,overdue:0});
  expect(await escalateAccessReviews(db,clock)).toEqual({dueSoon:0,overdue:0});
  const reviewerInbox=await listStaffNotifications(db,f.reviewer as unknown as WorkContext);
  expect(reviewerInbox).toEqual(expect.arrayContaining([expect.objectContaining({record_type:"iam_access_review",record_id:campaign.id,href:`/people/settings?section=reviews#${campaign.id}`,message:expect.stringContaining("due within 7 days")})]));
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(clock+1,f.reviewer.assignmentId).run();
  expect(await escalateAccessReviews(db,clock+7*86400000)).toEqual({dueSoon:0,overdue:1});
  expect(await escalateAccessReviews(db,clock+7*86400000)).toEqual({dueSoon:0,overdue:0});
  expect((await db.prepare("SELECT count(*)::int count FROM staff_notifications WHERE record_type='iam_access_review' AND record_id=? AND user_id=?").bind(campaign.id,f.reviewer.userId).first<number>("count"))).toBe(1);
  expect((await listStaffNotifications(db,f.reviewer as unknown as WorkContext)).some(notification=>notification.record_id===campaign.id)).toBe(false);
});
it("rejects a grant reviewed against an obsolete role revision", async () => {
  const f = await fixture();
  await expect(
    grantIamAccess(
      db,
      f.owner,
      grantInput.parse({
        userId: f.employee.userId,
        roleId: "employee",
        expectedRoleRevision: 1,
        scopeKind: "holding",
        reason: "Obsolete grant preview",
      }),
      key(),
    ),
  ).rejects.toMatchObject({ code: "ROLE_CHANGED" });
});
it("keeps the record inspector read-only and rejects mismatched actions", async () => {
  const f = await fixture();
  const { inspectIamAccess } = await import("@erpx/server-identity_access/public/iam-inspector");
  const result = await inspectIamAccess(db, f.owner, {
    userId: f.owner.userId,
    assignmentId: f.owner.assignmentId,
    permission: "finance.approve",
    recordType: "leave",
    recordId: key(),
  });
  expect(result.result).toBe("unavailable");
  expect(result.executionAuthorized).toBe(false);
  expect(
    (await db
      .prepare("SELECT count(*) n FROM iam_audit WHERE company_id=?")
      .bind(f.companyId)
      .first<{ n: number }>())!.n,
  ).toBe(0);
});

it("searches audit history beyond the first page without crossing company scope", async () => {
  const f = await fixture();
  for (let i = 0; i < 105; i++)
    await db
      .prepare("INSERT INTO iam_audit VALUES(?,?,?,?,?,?,?,?)")
      .bind(
        key(),
        f.companyId,
        f.owner.userId,
        f.owner.assignmentId,
        i === 0 ? "historic.event" : "recent.event",
        key(),
        "{}",
        now() + i,
      )
      .run();
  const first = await iamAuditHistory(db, f.owner, "");
  expect(first.items).toHaveLength(100);
  expect(first.nextCursor).not.toBeNull();
  const next = await iamAuditHistory(db, f.owner, "", first.nextCursor!);
  expect(next.items).toHaveLength(5);
  expect(next.nextCursor).toBeNull();
  expect(new Set([...first.items, ...next.items].map((r) => r.id)).size).toBe(
    105,
  );
  expect(
    (await iamAuditHistory(db, f.owner, "historic.event")).items,
  ).toHaveLength(1);
  await expect(
    iamAuditHistory(
      db,
      { ...f.owner, scopeKind: "branch", scopeNodeId: key() },
      "",
    ),
  ).rejects.toMatchObject({ code: "COMPANY_SCOPE_REQUIRED" });
});

it("batch role definitions match individual reads and observe publication or suspension immediately", async () => {
  const f = await fixture();
  const proposal = await saveIamRole(db,f.owner,roleInput.parse({name:"Batch role",expectedVersion:0,permissions:["finance.read","iam.self"],scopes:["holding"],reason:"Batch parity"}),key());
  await decideIamReview(db,f.reviewer,proposal.id,{expectedVersion:1,decision:"approve",reason:"Independent batch review"},key());
  const ids = (await db.prepare("SELECT id FROM iam_roles WHERE company_id IN ('*',?)").bind(f.companyId).all<{id:string}>()).results.map(r=>r.id);
  ids.push("missing-role");
  const batch = await iamRoles(db,f.companyId,ids);
  for (const id of ids) expect(batch.get(id)??null).toEqual(await iamRole(db,f.companyId,id));
  const custom = [...batch.values()].find(r=>r.name==="Batch role")!;
  expect(custom.permissions).toContain("finance.read");
  expect((await iamRoles(db,key(),[custom.id])).has(custom.id)).toBe(false);
  await db.prepare("UPDATE iam_roles SET status='archived' WHERE id=? AND company_id=?").bind(custom.id,f.companyId).run();
  expect((await iamRoles(db,f.companyId,[custom.id])).has(custom.id)).toBe(false);
  expect(await iamRole(db,f.companyId,custom.id)).toBeNull();
});

it("reads fallback person aliases within one company and observes member suspension on the next authority read", async () => {
  const f = await fixture(), alias = await f.user("employee"), otherCompany = key(), personId = key();
  await db.prepare("INSERT INTO hr_people(id,company_id,legal_name,country_code,status,user_id,created_by,created_at,updated_at) VALUES(?,?,?,'TH','active',?,?,?,?)")
    .bind(personId,f.companyId,"Authority snapshot person",f.employee.userId,f.owner.userId,now(),now()).run();
  await db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,login_policy) VALUES(?,?,?,?,'local')")
    .bind(f.companyId,alias.userId,`local:${alias.userId}`,personId).run();
  // An identically referenced person in another company must not enlarge this authority.
  await db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,login_policy) VALUES(?,?,?,?,'local')")
    .bind(otherCompany,"foreign-principal","local:foreign-principal",personId).run();
  const first = await loadIamAuthority(db,f.companyId,f.employee.userId,"employee");
  expect(first.personId).toBe(personId);
  expect(new Set(first.personUserIds)).toEqual(new Set([f.employee.userId,alias.userId]));
  await db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,login_policy,status) VALUES(?,?,?,'local','suspended')")
    .bind(f.companyId,f.employee.userId,`local:${f.employee.userId}`).run();
  await expect(loadIamAuthority(db,f.companyId,f.employee.userId,"employee")).rejects.toMatchObject({code:"IDENTITY_SUSPENDED"});
});


it("reads current workspace grants with their published roles and observes revoke, node withdrawal and Company isolation afresh", async () => {
  const f=await fixture(),other=await fixture(),branch=key(),grant=key(),clock=now();
  await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id) VALUES(?,'branch',?,'Snapshot branch',?,?)").bind(branch,branch,f.companyId,f.companyId).run();
  await db.prepare("INSERT INTO assignments(id,user_id,role,company_id,branch_id,scope_kind,scope_node_id,modules_json,valid_from,valid_until) VALUES(?,?,'employee',?,?,'branch',?,'[\"pms\"]',0,?)").bind(grant,f.employee.userId,f.companyId,branch,branch,clock+86400000).run();
  const first=await workspaceGrantsForAccount(db,f.companyId,f.employee.userId,clock);
  expect(first.rows.map(r=>r.id).sort()).toEqual([f.employee.assignmentId,grant].sort());
  expect(first.rows.find(r=>r.id===grant)).toMatchObject({branch_id:branch,scope_kind:'branch',modules_json:'["pms"]',scope_name:'Snapshot branch'});
  expect(first.roles).toEqual(await iamRoles(db,f.companyId,first.rows.map(r=>r.role)));
  expect((await workspaceGrantsForAccount(db,other.companyId,f.employee.userId)).rows).toEqual([]);
  await db.prepare("UPDATE organization_nodes SET status='archived' WHERE id=?").bind(branch).run();
  expect((await workspaceGrantsForAccount(db,f.companyId,f.employee.userId)).rows.map(r=>r.id)).toEqual([f.employee.assignmentId]);
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(now(),f.employee.assignmentId).run();
  const revoked=await workspaceGrantsForAccount(db,f.companyId,f.employee.userId);
  expect(revoked.rows).toEqual([]);expect(revoked.roles.size).toBe(0);
});

it("workspace grant definitions observe published role retirement without keeping prior authority", async () => {
  const f=await fixture();
  const proposal=await saveIamRole(db,f.owner,roleInput.parse({name:'Workspace snapshot role',expectedVersion:0,permissions:['finance.read','iam.self'],scopes:['holding'],reason:'Current published definitions'}),key());
  await decideIamReview(db,f.reviewer,proposal.id,{expectedVersion:1,decision:'approve',reason:'Independent published role'},key());
  const user=await f.user(proposal.roleId);
  expect((await workspaceGrantsForAccount(db,f.companyId,user.userId)).roles.get(proposal.roleId)?.permissions).toContain('finance.read');
  await db.prepare("UPDATE iam_roles SET status='archived' WHERE id=? AND company_id=?").bind(proposal.roleId,f.companyId).run();
  const next=await workspaceGrantsForAccount(db,f.companyId,user.userId);
  expect(next.rows).toHaveLength(1);expect(next.roles.has(proposal.roleId)).toBe(false);
});


it('reads Company-scoped session revocation afresh before any directory refresh',async()=>{
 const {accountDirectoryGate}=await import("@erpx/server-identity_access/public/iam-directory-refresh");
 const f=await fixture(),other=await fixture(),session=key();
 const initial=await accountDirectoryGate(db,f.companyId,f.employee.userId,session);
 expect(initial.revoked).toBe(false);await initial.refresh();
 await db.prepare('INSERT INTO iam_revoked_sessions(session_id,company_id,user_id,revoked_at) VALUES(?,?,?,?)').bind(session,other.companyId,f.employee.userId,now()).run();
 expect((await accountDirectoryGate(db,f.companyId,f.employee.userId,session)).revoked).toBe(false);
 await db.prepare('INSERT INTO iam_revoked_sessions(session_id,company_id,user_id,revoked_at) VALUES(?,?,?,?)').bind(session,f.companyId,f.employee.userId,now()).run();
 const revoked=await accountDirectoryGate(db,f.companyId,f.employee.userId,session);
 expect(revoked.revoked).toBe(true);await expect(revoked.refresh()).rejects.toMatchObject({code:'SESSION_REVOKED'});
 expect((await accountDirectoryGate(db,f.companyId,f.employee.userId,key())).revoked).toBe(false);
});

it('refreshes only stale active directory evidence and never contacts it after session revocation',async()=>{
 const {accountDirectoryGate}=await import("@erpx/server-identity_access/public/iam-directory-refresh");
 const directories=await import("@erpx/server-identity_access/public/directory-synchronization");
 const synchronize=vi.spyOn(directories,'synchronizeConnection').mockRejectedValue(new Error('mock directory outage'));
 try {
 const f=await fixture(),other=await fixture(),session=key();
 const saved=await saveConnection(db,f.owner,connectionInput.parse({expectedVersion:0,name:'Readiness evidence',provider:'univention',config:{mode:'ldaps',secretProfile:'ucs',authority:'ucs.corp',ldapUrl:'ldaps://ucs.corp',baseDn:'dc=corp'}}),key());
 await db.prepare("UPDATE iam_connections SET status='active' WHERE id=?").bind(saved.id).run();
 const connection=await connectionById(db,f.companyId,saved.id);
 await applyDirectorySnapshot(db,connection,{complete:true,groups:[],users:[{externalId:key(),username:'readiness',name:'Mock staff',email:null,enabled:true,groupIds:[]}]});
 const identity=await db.prepare('SELECT id FROM iam_identities WHERE connection_id=?').bind(saved.id).first<{id:string}>();
 await db.prepare('UPDATE iam_identities SET user_id=?,observed_at=? WHERE id=?').bind(f.employee.userId,now(),identity!.id).run();
 await db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,login_policy,connection_id,identity_id) VALUES(?,?,?,'directory',?,?)").bind(f.companyId,f.employee.userId,key(),saved.id,identity!.id).run();
 await (await accountDirectoryGate(db,f.companyId,f.employee.userId,session)).refresh();expect(synchronize).not.toHaveBeenCalled();
 await db.prepare('UPDATE iam_identities SET observed_at=? WHERE id=?').bind(now()-300001,identity!.id).run();
 await (await accountDirectoryGate(db,other.companyId,f.employee.userId,session)).refresh();expect(synchronize).not.toHaveBeenCalled();
 const stale=await accountDirectoryGate(db,f.companyId,f.employee.userId,session);
 await expect(stale.refresh()).rejects.toMatchObject({code:'DIRECTORY_ACCESS_UNAVAILABLE'});expect(synchronize).toHaveBeenCalledTimes(1);
 expect(synchronize.mock.calls[0][1]).toMatchObject({id:saved.id,company_id:f.companyId,status:'active'});
 await db.prepare('INSERT INTO iam_revoked_sessions(session_id,company_id,user_id,revoked_at) VALUES(?,?,?,?)').bind(session,f.companyId,f.employee.userId,now()).run();
 const revoked=await accountDirectoryGate(db,f.companyId,f.employee.userId,session);
 expect(revoked.revoked).toBe(true);await expect(revoked.refresh()).rejects.toMatchObject({code:'SESSION_REVOKED'});expect(synchronize).toHaveBeenCalledTimes(1);
 } finally {synchronize.mockRestore();}
});

it('combines fresh session readiness with current published account grants in one PostgreSQL snapshot',async()=>{
 const {accountDirectoryGate}=await import("@erpx/server-identity_access/public/iam-directory-refresh");
 const {measureReads}=await import("@erpx/server-runtime/adapters/runtime/read-metrics");
 const f=await fixture(),other=await fixture(),session=key(),clock=now();
 // Reflect lets the RED run exercise the original four-argument adapter before
 // the optional internal projection exists; no fake database or authority.
 const read=()=>measureReads(async()=>{
  const gate=await Reflect.apply(accountDirectoryGate,undefined,[db,f.companyId,f.employee.userId,session,{includeAccountGrants:true,observedNow:clock}]);
  await gate.refresh();return gate;
 });
 const first=await read();expect(first.metrics.queries).toBe(1);expect(first.result.revoked).toBe(false);
 expect(first.result.accountGrants?.rows.map((r:{id:string})=>r.id)).toEqual([f.employee.assignmentId]);
 expect(first.result.accountGrants!.roles).toEqual(await iamRoles(db,f.companyId,['employee']));
 await db.prepare('INSERT INTO iam_revoked_sessions(session_id,company_id,user_id,revoked_at) VALUES(?,?,?,?)').bind(session,other.companyId,f.employee.userId,now()).run();
 expect((await read()).result.accountGrants!.rows.map((r:{id:string})=>r.id)).toEqual([f.employee.assignmentId]);
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(now(),f.employee.assignmentId).run();
 const withdrawn=await read();expect(withdrawn.metrics.queries).toBe(1);expect(withdrawn.result.accountGrants!.rows).toEqual([]);expect(withdrawn.result.accountGrants!.roles.size).toBe(0);
 await db.prepare('INSERT INTO iam_revoked_sessions(session_id,company_id,user_id,revoked_at) VALUES(?,?,?,?)').bind(session,f.companyId,f.employee.userId,now()).run();
 const revoked=await Reflect.apply(accountDirectoryGate,undefined,[db,f.companyId,f.employee.userId,session,{includeAccountGrants:true,observedNow:clock}]);
 expect(revoked.revoked).toBe(true);expect(revoked.accountGrants!.rows).toEqual([]);await expect(revoked.refresh()).rejects.toMatchObject({code:'SESSION_REVOKED'});
});

it('combined account authority observes node withdrawal, published role changes and suspended members without retaining prior grants',async()=>{
 const {accountDirectoryGate}=await import("@erpx/server-identity_access/public/iam-directory-refresh");
 const f=await fixture(),other=await fixture(),session=key(),branch=key(),branchGrant=key();
 const read=()=>accountDirectoryGate(db,f.companyId,f.employee.userId,session,{includeAccountGrants:true});
 await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id) VALUES(?,'branch',?,'Current branch',?,?)").bind(branch,branch,f.companyId,f.companyId).run();
 await db.prepare("INSERT INTO assignments(id,user_id,role,company_id,branch_id,scope_kind,scope_node_id,modules_json,valid_from,valid_until) VALUES(?,?,'employee',?,?,'branch',?,'[\"pms\"]',0,?)").bind(branchGrant,f.employee.userId,f.companyId,branch,branch,now()+86400000).run();
 expect((await read()).accountGrants!.rows.map(r=>r.id).sort()).toEqual([f.employee.assignmentId,branchGrant].sort());
 expect((await accountDirectoryGate(db,other.companyId,f.employee.userId,session,{includeAccountGrants:true})).accountGrants!.rows).toEqual([]);
 await db.prepare("UPDATE organization_nodes SET status='archived' WHERE id=?").bind(branch).run();
 expect((await read()).accountGrants!.rows.map(r=>r.id)).toEqual([f.employee.assignmentId]);
 const reason='Independently reviewed current account snapshot role';
 const first=await saveIamRole(db,f.owner,roleInput.parse({name:'Combined snapshot role',expectedVersion:0,permissions:['finance.read','iam.self'],scopes:['holding'],reason}),key());
 await decideIamReview(db,f.reviewer,first.id,{expectedVersion:1,decision:'approve',reason},key());
 const user=await f.user(first.roleId),customRead=()=>accountDirectoryGate(db,f.companyId,user.userId,session,{includeAccountGrants:true});
 expect((await customRead()).accountGrants!.roles.get(first.roleId)?.permissions).toContain('finance.read');
 const changed=await saveIamRole(db,f.owner,roleInput.parse({id:first.roleId,name:'Combined snapshot role',expectedVersion:1,permissions:['records.read','iam.self'],scopes:['holding'],reason}),key());
 await decideIamReview(db,f.reviewer,changed.id,{expectedVersion:1,decision:'approve',reason},key());
 const fresh=(await customRead()).accountGrants!;expect(fresh.roles.get(first.roleId)?.revision).toBe(2);expect(fresh.roles.get(first.roleId)?.permissions).not.toContain('finance.read');expect(fresh.roles.get(first.roleId)?.permissions).toContain('records.read');
 await db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,login_policy,status) VALUES(?,?,?,'local','suspended')").bind(f.companyId,user.userId,`local:${user.userId}`).run();
 expect((await customRead()).accountGrants!.rows).toEqual([]);
 await expect(loadIamAuthority(db,f.companyId,user.userId,first.roleId,session)).rejects.toMatchObject({code:'IDENTITY_SUSPENDED'});
});

it('discards stale account grants and rereads after directory refresh while failing closed on outage, unchanged evidence and refresh-time revocation',async()=>{
 const {accountDirectoryGate}=await import("@erpx/server-identity_access/public/iam-directory-refresh");
 const {measureReads}=await import("@erpx/server-runtime/adapters/runtime/read-metrics");
 const directories=await import("@erpx/server-identity_access/public/directory-synchronization");
 const f=await fixture(),session=key();
 const saved=await saveConnection(db,f.owner,connectionInput.parse({expectedVersion:0,name:'Combined refresh evidence',provider:'univention',config:{mode:'ldaps',secretProfile:'ucs',authority:'ucs.corp',ldapUrl:'ldaps://ucs.corp',baseDn:'dc=corp'}}),key());
 await db.prepare("UPDATE iam_connections SET status='active' WHERE id=?").bind(saved.id).run();
 const connection=await connectionById(db,f.companyId,saved.id);
 await applyDirectorySnapshot(db,connection,{complete:true,groups:[],users:[{externalId:key(),username:'snapshot-refresh',name:'Mock staff',email:null,enabled:true,groupIds:[]}]});
 const identity=(await db.prepare('SELECT id FROM iam_identities WHERE connection_id=?').bind(saved.id).first<{id:string}>())!;
 const age=()=>db.prepare('UPDATE iam_identities SET user_id=?,observed_at=? WHERE id=?').bind(f.employee.userId,now()-300001,identity.id).run();await age();
 await db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,login_policy,connection_id,identity_id) VALUES(?,?,?,'directory',?,?)").bind(f.companyId,f.employee.userId,key(),saved.id,identity.id).run();
 let outcome:'updated'|'outage'|'unchanged'|'revoked'='updated';
 const synchronize=vi.spyOn(directories,'synchronizeConnection').mockImplementation(async()=>{
  if(outcome==='outage')throw new Error('mock directory outage');
  if(outcome==='unchanged')return;
  await db.prepare('UPDATE iam_identities SET observed_at=? WHERE id=?').bind(now(),identity.id).run();
  if(outcome==='revoked')await db.prepare('INSERT INTO iam_revoked_sessions(session_id,company_id,user_id,revoked_at) VALUES(?,?,?,?)').bind(session,f.companyId,f.employee.userId,now()).run();
  else await db.prepare("UPDATE assignments SET modules_json='[\"pms\",\"pos\"]' WHERE id=?").bind(f.employee.assignmentId).run();
 });
 const read=()=>accountDirectoryGate(db,f.companyId,f.employee.userId,session,{includeAccountGrants:true});
 try{
  const initial=await measureReads(read);expect(initial.metrics.queries).toBe(1);expect(initial.result.accountGrants!.rows).toEqual([]);expect(initial.result.accountGrants!.roles.size).toBe(0);
  const refreshed=await measureReads(()=>initial.result.refresh());expect(refreshed.metrics.queries).toBe(3);
  expect(initial.result.accountGrants!.rows.map(r=>r.id)).toEqual([f.employee.assignmentId]);expect(initial.result.accountGrants!.rows[0].modules_json).toBe('["pms","pos"]');
  await age();outcome='outage';const failed=await read();await expect(failed.refresh()).rejects.toMatchObject({code:'DIRECTORY_ACCESS_UNAVAILABLE'});expect(failed.accountGrants!.rows).toEqual([]);
  outcome='unchanged';const unchanged=await read();await expect(unchanged.refresh()).rejects.toMatchObject({code:'DIRECTORY_ACCESS_UNAVAILABLE'});expect(unchanged.accountGrants!.rows).toEqual([]);
  outcome='revoked';const revokedDuring=await read();await expect(revokedDuring.refresh()).rejects.toMatchObject({code:'SESSION_REVOKED'});expect(revokedDuring.accountGrants!.rows).toEqual([]);
  const alreadyRevoked=await read();expect(alreadyRevoked.revoked).toBe(true);await expect(alreadyRevoked.refresh()).rejects.toMatchObject({code:'SESSION_REVOKED'});expect(synchronize).toHaveBeenCalledTimes(4);
 }finally{synchronize.mockRestore();}
});


// Included Expense/Payroll pages reuse real independent IAM publication and
// grants; the financial assignment receives no People module or Directory right.
// Imported Organization/worker references follow fixtures/expenses/company.ts.
// All new compensation, references, mapping, calculate, independent review and
// economic posting below use their original owner commands. This is a financial
// entry/read parity fixture, not complete Company/Directory startup evidence.
async function postFinancialEntryPayrollSource(original:WorkContext,reviewer:WorkContext,employee:IamActor,payrollPreparer:WorkContext,payrollApprover:WorkContext,payGroupId:string,runId:string){
 const people=await import('@erpx/server-people_payroll/public/people-payroll'),finance=await import('@erpx/server-finance/public/finance'),money=await import('@erpx/server-finance/public/company-money'),{resolveFlow,assignFlowPrimary}=await import('@erpx/server-workflow/public/flow-routing'),{submitPayrollReview}=await import('@erpx/server-people_payroll/public/payroll-readiness');
 const branch=key(),level=key(),engagementId=`eng-${employee.userId}`;
 // Existing imported worker/reference fixture only. Never insert a Payroll
 // item, compensation approval, journal, balance or posted run/status by SQL.
 await db.batch([
  db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Imported financial-entry payroll branch',?,?,?)").bind(branch,branch,original.companyId,original.companyId,branch),
  db.prepare("INSERT INTO staff_levels(id,company_id,code,name,rank,created_at,updated_at) VALUES(?,?,'6','Imported payroll worker level',6,1,1)").bind(level,original.companyId),
  ...[original,reviewer,employee,payrollPreparer,payrollApprover].flatMap(actor=>[
   db.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,'TH','fixture',1,1)").bind(`person-${actor.userId}`,actor.userId,original.companyId,`Imported financial-entry ${actor.userId}`),
   db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,worker_type,employment_type,status,start_date,job_title,currency,pay_group_id,created_by,created_at,updated_at,level_id) VALUES(?,?,?,?,'employee','salaried','active','2026-09-01','Imported payroll reference worker','THB',?,'fixture',1,1,?)").bind(`eng-${actor.userId}`,`person-${actor.userId}`,original.companyId,branch,actor.userId===employee.userId?payGroupId:null,level),
  ]),
 ]);
 original.iam=await loadIamAuthority(db,original.companyId,original.userId,original.role,original.authority?.sessionId);reviewer.iam=await loadIamAuthority(db,reviewer.companyId,reviewer.userId,reviewer.role,reviewer.authority?.sessionId);
 payrollPreparer.iam=await loadIamAuthority(db,payrollPreparer.companyId,payrollPreparer.userId,payrollPreparer.role,payrollPreparer.authority?.sessionId);payrollApprover.iam=await loadIamAuthority(db,payrollApprover.companyId,payrollApprover.userId,payrollApprover.role,payrollApprover.authority?.sessionId);
 // Original legacy Finance reference constructors, as used by Payroll owner
 // fixtures. The financial source role below receives no generic Finance grant.
 const priorFinanceMode=process.env.ERPX_FINANCE_PACKAGE_ENFORCEMENT;
 vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','disabled');
 let chartId:string,expenseAccountId:string,payableAccountId:string,periodId:string;
 const receiptId=(value:unknown)=>{if(!value||typeof value!=='object'||!('id'in value)||typeof value.id!=='string'||!value.id)throw Error('ORIGINAL_PAYROLL_REFERENCE_RECEIPT_REQUIRED');return value.id;};
 const versionedReceipt=(value:unknown)=>{const id=receiptId(value);if(!value||typeof value!=='object'||!('version'in value)||typeof value.version!=='number'||!Number.isSafeInteger(value.version)||value.version<1)throw Error('ORIGINAL_PAYROLL_VERSIONED_RECEIPT_REQUIRED');return {id,version:value.version};};
 try{
  chartId=receiptId(await finance.createChart(original,finance.createChartSchema.parse({name:'Native financial-entry payroll chart',baseCurrency:'THB'}),key()));
  expenseAccountId=receiptId(await finance.createAccount(original,finance.createAccountSchema.parse({chartId,code:'5100',name:'Native payroll wage expense',category:'expense',currency:'THB'}),key()));
  payableAccountId=receiptId(await finance.createAccount(original,finance.createAccountSchema.parse({chartId,code:'2100',name:'Native payroll net payable',category:'liability',currency:'THB'}),key()));
  periodId=receiptId(await finance.createPeriod(original,finance.createPeriodSchema.parse({name:'Actual financial-entry September period',startDate:'2026-09-01',endDate:'2026-09-30'}),key()));
  await money.confirmManualRate(original,money.manualRateSchema.parse({currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36',note:'Explicit isolated Payroll reference quote; not a market-rate assertion'}),key());
 }finally{vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT',priorFinanceMode);}
 const compensation=versionedReceipt(await people.createCompensation(original,people.compensationSchema.parse({engagementId,effectiveFrom:'2026-09-01',payFrequency:'monthly',baseAmount:'1000',currency:'THB',components:[]}),key()));
 await people.transitionCompensation(reviewer,compensation.id,{status:'approved',expectedVersion:compensation.version},key());
 for(const [stepKey,userId]of [['calculate',payrollPreparer.userId],['preview',payrollPreparer.userId],['in_review',payrollApprover.userId],['approved',reviewer.userId]]as const){const flow=await resolveFlow(db,original,'payroll',runId);await assignFlowPrimary(db,original,'payroll',runId,{stepKey,userId,expectedVersion:flow.version,recordVersion:flow.recordVersion,reason:'Explicit original Company Payroll flow with independent review',saveDefault:false},key());}
 const calculated=versionedReceipt(await people.calculatePayrollRun(payrollPreparer,runId,key())),read=await people.getPayrollRun(original,runId);
 if(typeof read.run.input_digest!=='string')throw Error('ACTUAL_CALCULATED_PAYROLL_DIGEST_REQUIRED');
 const submitted=versionedReceipt(await submitPayrollReview(payrollPreparer,runId,{expectedVersion:calculated.version,inputDigest:read.run.input_digest},key())),approved=versionedReceipt(await people.approvePayrollRun(payrollApprover,runId,submitted.version,key()));
 const mapping=versionedReceipt(await people.savePayrollAccountMapping(original,payGroupId,people.payrollAccountMappingSchema.parse({chartId,expenseAccountId,netPayableAccountId:payableAccountId}),key()));
 await people.approvePayrollAccountMapping(reviewer,mapping.id,mapping.version,key());
 const posted=await people.postPayroll(reviewer,runId,people.payrollPostSchema.parse({accountMappingId:mapping.id,periodId,entryDate:'2026-09-30',expectedVersion:approved.version}),key());
 expect(posted).toMatchObject({id:runId,status:'posted',netPayableMinor:100000});
 expect((await people.getPayrollRun(original,runId)).run.status).toBe('posted');
}
async function financialProductEntryFixture(postedPayroll=false){
 const f=await fixture(),grantReviewer=await f.user(),{packageGrantStore,readCompanyPurchaseFacts}=await import('@erpx/server-commercial/public/package-grants'),{ownerPackageObligations,withOwnerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),{packageGrantRevision}=await import('@erpx/server-commercial/public/package-grants');
 const store=packageGrantStore(env.H001_DB,{authorize:async(_database,companyId)=>{if(companyId!==f.companyId)throw Error('FINANCIAL_ENTRY_FIXTURE_COMPANY');return {userId:'fictional-financial-entry-provider'};},obligations:(transaction,companyId,packageId)=>ownerPackageObligations(env.H001_DB,transaction,companyId,packageId)});
 const permissions=['module.finance.read','finance.pay','payroll.payment.read','expense.pay','expense.review.read','expense.audit.read','workflow.treasury','iam.self'];
 const role=await saveIamRole(db,f.owner,roleInput.parse({name:'Native financial settlement only',expectedVersion:0,permissions,scopes:['holding'],reason:'Read and settle original financial owner records without Directory authority'}),key());
 await decideIamReview(db,f.reviewer,role.id,{expectedVersion:1,decision:'approve',reason:'Independent bounded financial permission review'},key());
 const proposed=await grantIamAccess(db,f.reviewer,grantInput.parse({userId:f.owner.userId,roleId:role.roleId,scopeKind:'holding',validUntil:now()+86400000,reason:'Explicit reviewed financial work role'}),key());
 await decideIamReview(db,grantReviewer,proposed.id,{expectedVersion:1,decision:'approve',reason:'Independent financial assignment review'},key());
 f.owner.iam=await loadIamAuthority(db,f.companyId,f.owner.userId,f.owner.role,f.owner.sessionId);
 const roleAssignment=(await workspaceGrantsForAccount(env.H001_DB,f.companyId,f.owner.userId)).rows.find(row=>row.role===role.roleId)!;
 const sourceScope={companyId:f.companyId,branchId:null,departmentId:null,userId:f.owner.userId,assignmentId:roleAssignment.id,assignmentVersion:roleAssignment.version,role:role.roleId,sessionId:f.owner.sessionId};
 const {peopleFinancialProductEntry}=await import('@erpx/server-people_payroll/public/financial-product-entry');
 expect(await peopleFinancialProductEntry(env.H001_DB,sourceScope,{kind:'payroll'})).toBeNull();
 expect(await peopleFinancialProductEntry(env.H001_DB,sourceScope,{kind:'expense'})).toBeNull();
 for(const packageId of ['people','people-payroll','people-expenses'])await store.apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:f.companyId,packageId,packageVersion:1,state:'active',startsAt:now()-1000,endsAt:null,version:1,reason:'Required People parent and financial add-ons',evidence:'test:actual-native-financial-entry'}});
 const scope={kind:'holding' as const,holdingId:'h001' as const,holdingSlug:'rehearsal',holdingName:'IAM rehearsal',companyId:f.companyId};
 const work=(actor:IamActor):WorkContext=>({...actor,holdingId:'h001',name:'Native financial entry actor',email:`${actor.userId}@example.test`,holdingSlug:'rehearsal',holdingName:'IAM rehearsal',scope,assignmentScope:'holding',inherited:false,modules:[],assignments:[],workspaces:[]});
 const original=work(f.owner),{createPayGroup,payGroupSchema,createPayrollRun,payrollRunSchema,getPayrollRun,listPayrollRuns}=await import('@erpx/server-people_payroll/public/people-payroll'),{saveExpense,expenseUpdateSchema,getExpense,listExpenses}=await import('@erpx/server-people_payroll/public/expenses');
 const group=await createPayGroup(original,payGroupSchema.parse({name:'Native entry-only payroll group',countryCode:'TH',currency:'THB',frequency:'monthly',timezone:'Asia/Bangkok'}),key());
 const run=await createPayrollRun(original,payrollRunSchema.parse({payGroupId:group.id,periodStart:'2026-09-01',periodEnd:'2026-09-30',payDate:'2026-10-01',currency:'THB'}),key());
 if(postedPayroll){
  // Workflow candidates require their original Payroll roles, not the Finance
  // owners' wider permissions. Separate existing accounts receive independently
  // reviewed original grants and explicitly select those actual assignments.
  const payrollActor=async(role:'payroll_preparer'|'payroll_approver'):Promise<WorkContext>=>{
   const identity=await f.user('employee'),proposal=await grantIamAccess(db,f.owner,grantInput.parse({userId:identity.userId,roleId:role,scopeKind:'holding',validUntil:now()+86400000,reason:'Original independent Payroll role for native financial source fixture'}),key());
   await decideIamReview(db,f.reviewer,proposal.id,{expectedVersion:1,decision:'approve',reason:'Independent original Payroll work assignment review'},key());
   const assignment=(await workspaceGrantsForAccount(env.H001_DB,f.companyId,identity.userId)).rows.find(row=>row.role===role);
   if(!assignment||typeof assignment.version!=='number'||!Number.isSafeInteger(assignment.version)||assignment.version<1)throw Error('CURRENT_NATIVE_PAYROLL_ASSIGNMENT_REQUIRED');
   const {readWorkSelection}=await import('@erpx/server-work_projection/public/work-role-store'),prior=await readWorkSelection(env.H001_DB,identity.sessionId,identity.userId,f.companyId);
   if(!prior)throw Error('ORIGINAL_PAYROLL_WORK_SELECTION_REQUIRED');
   const selected=await selectWorkRole(env.H001_DB,{sessionId:identity.sessionId,userId:identity.userId,companyId:f.companyId,assignmentId:assignment.id,assignmentVersion:assignment.version,revision:prior.revision,expiresAt:now()+600000});
   if(typeof selected.assignment_version!=='number'||!Number.isSafeInteger(selected.assignment_version)||selected.assignment_version<1)throw Error('SELECTED_NATIVE_PAYROLL_ASSIGNMENT_REQUIRED');
   return {...work(identity),role,assignmentId:assignment.id,modules:JSON.parse(assignment.modules_json) as string[],authority:{sessionId:identity.sessionId,assignmentVersion:selected.assignment_version,revision:selected.revision},iam:await loadIamAuthority(db,f.companyId,identity.userId,role,identity.sessionId)};
  };
  const payrollPreparer=await payrollActor('payroll_preparer'),payrollApprover=await payrollActor('payroll_approver'),reviewer=work(f.reviewer);
  await postFinancialEntryPayrollSource(original,reviewer,f.employee,payrollPreparer,payrollApprover,group.id,run.id);
  if(!original.iam||!reviewer.iam)throw Error('CURRENT_IMPORTED_WORKER_AUTHORITY_REQUIRED');f.owner.iam=original.iam;f.reviewer.iam=reviewer.iam;f.employee.iam=await loadIamAuthority(db,f.companyId,f.employee.userId,f.employee.role,f.employee.sessionId);
 }
 const expenseResult=await saveExpense(original,null,expenseUpdateSchema.parse({expectedVersion:1,merchant:'Native financial entry source',businessPurpose:'Owner-created draft read boundary'}),key());
 if(typeof expenseResult.id!=='string'||!expenseResult.id)throw Error('NATIVE_EXPENSE_RECEIPT_ID_REQUIRED');const expense={...expenseResult,id:expenseResult.id};
 const hiddenResult=await saveExpense(work(f.employee),null,expenseUpdateSchema.parse({expectedVersion:1,merchant:'Private employee draft',businessPurpose:'Another native claimant source'}),key());
 if(typeof hiddenResult.id!=='string'||!hiddenResult.id)throw Error('NATIVE_PRIVATE_EXPENSE_RECEIPT_ID_REQUIRED');const hidden={...hiddenResult,id:hiddenResult.id};
 const {accountWorkModuleAccess}=await import('@erpx/server-identity_access/public/work-module-access'),{createProductWorkspace}=await import('@erpx/server-identity_access/application/product-workspace'),{productWorkspaceTarget}=await import('@erpx/server-identity_access/domain/product-workspace'),{initializeWorkSelection,readWorkSelection}=await import('@erpx/server-work_projection/public/work-role-store');
 const access=async()=>{const {rows,roles}=await workspaceGrantsForAccount(env.H001_DB,f.companyId,f.owner.userId);return {scope,userId:f.owner.userId,sessionId:f.owner.sessionId,expiresAt:now()+600000,name:'Native financial worker',email:'native-financial@example.test',requestHeaders:new Headers(),assignments:rows.filter(row=>roles.get(row.role)?.scopes.includes(row.scope_kind)).map(row=>({id:row.id,role:row.role,companyId:row.company_id,branchId:row.branch_id,scopeKind:row.scope_kind,scopeNodeId:row.scope_node_id,modules:JSON.parse(row.modules_json) as string[],version:row.version}))};};
 const entry=(target:{kind:'expense'|'payroll';id?:string})=>peopleFinancialProductEntry(env.H001_DB,sourceScope,target);
 let selects=0;
 const service=createProductWorkspace({access,modules:accountWorkModuleAccess,purchaseFacts:account=>readCompanyPurchaseFacts(env.H001_DB,account.scope.companyId),ready:async()=>true,peopleFinancialEntry:(account,assignment,target)=>peopleFinancialProductEntry(env.H001_DB,{companyId:account.scope.companyId,branchId:null,departmentId:null,userId:account.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:account.sessionId},target),selection:(account,applicable)=>initializeWorkSelection(env.H001_DB,{sessionId:account.sessionId,userId:account.userId,companyId:account.scope.companyId,expiresAt:account.expiresAt},account.assignments,applicable),select:(account,option,revision)=>{selects++;return selectWorkRole(env.H001_DB,{sessionId:account.sessionId,userId:account.userId,companyId:account.scope.companyId,expiresAt:account.expiresAt,assignmentId:option.assignmentId,assignmentVersion:option.assignmentVersion,revision});},now,error:(status,code,message)=>new (class extends Error{constructor(){super(message);Object.assign(this,{status,code});}})()});
 const otherProposal=await grantIamAccess(db,f.reviewer,grantInput.parse({userId:f.owner.userId,roleId:'finance_preparer',scopeKind:'holding',validUntil:now()+86400000,reason:'Separate live accounting role must remain selected until an explicit choice'}),key());
 await decideIamReview(db,grantReviewer,otherProposal.id,{expectedVersion:1,decision:'approve',reason:'Independent separate accounting role review'},key());
 const otherAssignment=(await workspaceGrantsForAccount(env.H001_DB,f.companyId,f.owner.userId)).rows.find(row=>row.role==='finance_preparer')!;
 await selectWorkRole(env.H001_DB,{sessionId:f.owner.sessionId,userId:f.owner.userId,companyId:f.companyId,assignmentId:otherAssignment.id,assignmentVersion:otherAssignment.version,revision:1,expiresAt:now()+600000});
 const financialActor=async():Promise<WorkContext>=>{const current=await readWorkSelection(env.H001_DB,f.owner.sessionId,f.owner.userId,f.companyId);if(!current||current.assignment_id!==roleAssignment.id||typeof current.assignment_version!=='number'||!Number.isSafeInteger(current.assignment_version)||current.assignment_version<1)throw Error('CURRENT_FINANCIAL_ASSIGNMENT_VERSION_REQUIRED');return {...original,assignmentId:roleAssignment.id,role:role.roleId,modules:['finance'],authority:{sessionId:f.owner.sessionId,assignmentVersion:current.assignment_version,revision:current.revision},iam:await loadIamAuthority(env.H001_DB,f.companyId,f.owner.userId,role.roleId,f.owner.sessionId)};};
 return {...f,original,store,role,roleAssignment,scope,access,sourceScope,entry,service,productWorkspaceTarget,run,expense,hidden,financialActor,getPayrollRun,listPayrollRuns,getExpense,listExpenses,selects:()=>selects,readCompanyPurchaseFacts,withOwnerPackageObligations,packageGrantRevision};
}
it('preserves actual Company financial-role Apps/SSO entry and original redacted owner readers without People module or base Finance purchase',async()=>{
 const f=await financialProductEntryFixture(true),{accountWorkModuleAccess}=await import('@erpx/server-identity_access/public/work-module-access'),{createProductLauncher}=await import('@erpx/server-work_projection/application/product-launcher'),{buildWorkspaceHoldingChoice}=await import('@erpx/feature-work_projection/lib/workspace-choice'),{peopleFinancialProductEntry}=await import('@erpx/server-people_payroll/public/financial-product-entry');
 const account=await f.access(),authority=await accountWorkModuleAccess(account),before=(await db.prepare('SELECT id,role,version,modules_json FROM assignments WHERE company_id=? ORDER BY id').bind(f.companyId).all()).results;
 expect(authority.modulesByAssignment.get(f.roleAssignment.id)).toEqual(['finance']);expect(authority.permissionsByAssignment.get(f.roleAssignment.id)).not.toContain('module.people_payroll.read');
 for(const target of [{kind:'expense' as const,id:f.expense.id},{kind:'payroll' as const,id:f.run.id}])expect(await f.entry(target)).toMatchObject({purchasePurpose:'history'});
 expect(await f.entry({kind:'expense',id:f.hidden.id})).toBeNull();expect(await f.entry({kind:'payroll',id:'missing-native-run'})).toBeNull();
 const choices=[buildWorkspaceHoldingChoice('rehearsal','IAM rehearsal',[{companyId:f.companyId,companyName:'Financial Company',scopeKind:'holding',scopeNodeId:null}],[],true)!];
 const links=await createProductLauncher({choices:async()=>choices,access:async()=>account,purchaseFacts:access=>f.readCompanyPurchaseFacts(env.H001_DB,access.scope.companyId),moduleAccess:async()=>({...authority,modulesByAssignment:new Map([[f.roleAssignment.id,authority.modulesByAssignment.get(f.roleAssignment.id)!]]),presentationModulesByAssignment:new Map([[f.roleAssignment.id,authority.presentationModulesByAssignment.get(f.roleAssignment.id)!]]),permissionsByAssignment:new Map([[f.roleAssignment.id,authority.permissionsByAssignment.get(f.roleAssignment.id)!]])}),ready:async()=>true,peopleFinancialEntry:(access,assignment,target)=>peopleFinancialProductEntry(env.H001_DB,{...f.sourceScope,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role},target),companySlug:async()=>null,origin:id=>`https://${id}-dev-erpx.jesadakorn.com`,denied:()=>false,now})();
 expect(links.map(row=>row.productId)).toEqual(['finance','people']);
 for(const link of links){const sso=new URL(link.href),launch=new URL(sso.searchParams.get('next')!,sso.origin);expect(launch.searchParams.get('next')).toBe('/holdings/rehearsal/'+(link.productId==='finance'?'finance/documents/expenses':'people-organization/people/payroll/dashboard'));}
 const input=f.productWorkspaceTarget('people',f.companyId,`/holdings/rehearsal/people-organization/people/payroll/runs/${f.run.id}?q=payment#source`)!,state=await f.service.read('people',input);
 expect(state.target).toBeNull();expect(state.options.some(option=>option.assignmentId===f.roleAssignment.id&&option.sourceRecord)).toBe(true);expect(f.selects()).toBe(0);
 await expect(f.service.choose('people',input,f.roleAssignment.id,state.selection!.revision,2)).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});
 await f.service.choose('people',input,f.roleAssignment.id,state.selection!.revision,1);expect((await f.service.read('people',input)).target).toBe(input.target);
 const actor=await f.financialActor(),read=await f.getPayrollRun(actor,String(f.run.id));expect(read.items).toEqual([]);expect(read.exports).toEqual([]);expect(read.corrections).toEqual([]);expect(read.run.totals_json).toBe('{}');expect((await f.listPayrollRuns(actor)).some(row=>row.id===f.run.id)).toBe(true);
 expect((await f.getExpense(actor,String(f.expense.id))).id).toBe(f.expense.id);expect((await f.listExpenses(actor)).some(row=>row.id===f.expense.id)).toBe(true);await expect(f.getExpense(actor,String(f.hidden.id))).rejects.toMatchObject({status:404});
 for(const [audience,path]of [['people','people-organization/people/directory'],['finance','finance/documents']]as const){const ordinary=f.productWorkspaceTarget(audience,f.companyId,'/holdings/rehearsal/'+path)!,current=await f.service.read(audience,ordinary).catch(()=>null);expect(current?.options.some(option=>option.assignmentId===f.roleAssignment.id)??false).toBe(false);}
 expect((await db.prepare('SELECT id,role,version,modules_json FROM assignments WHERE company_id=? ORDER BY id').bind(f.companyId).all()).results).toEqual(before);
});
it('rechecks financial source Company, scope, purchase history, release, IAM version and session without replacing the live assignment',async()=>{
 const f=await financialProductEntryFixture(),{peopleFinancialProductEntry}=await import('@erpx/server-people_payroll/public/financial-product-entry'),{readCompanyPackageGrants}=await import('@erpx/server-commercial/public/package-grants'),{readWorkSelection}=await import('@erpx/server-work_projection/public/work-role-store'),target={kind:'payroll' as const,id:String(f.run.id)};
 for(const patch of [{companyId:'foreign-company'},{branchId:'foreign-branch'},{departmentId:'foreign-department'},{beta:true},{assignmentVersion:f.roleAssignment.version+1},{sessionId:'revoked-financial-entry-session'}]){
  if('sessionId'in patch)await db.prepare('INSERT INTO iam_revoked_sessions(session_id,company_id,user_id,revoked_at) VALUES(?,?,?,?)').bind(patch.sessionId,f.companyId,f.owner.userId,now()).run();
  expect(await peopleFinancialProductEntry(env.H001_DB,{...f.sourceScope,...patch},target)).toBeNull();
 }
 const image=(await db.prepare('SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id').bind(f.companyId).all()).results,current=await readWorkSelection(env.H001_DB,f.owner.sessionId,f.owner.userId,f.companyId);
 const expiring=await readCompanyPackageGrants(env.H001_DB,f.companyId),endsAt=now()-1,expiryInput={companyId:f.companyId,expectedRevision:f.packageGrantRevision(f.companyId,expiring),idempotencyKey:key(),changes:expiring.filter(grant=>grant.packageId==='people-payroll'||grant.packageId==='people-expenses').map(grant=>({expectedVersion:grant.version,grant:{...grant,endsAt,version:grant.version+1,reason:'Expire the actual addon periods with their active People parent retained',evidence:'test:financial-entry-addon-expiry'}}))};
 const expiryPreview=await f.store.batch.preview(expiryInput);await f.store.batch.apply(expiryInput,async actual=>{expect(actual).toEqual(expiryPreview);});
 const {readPurchasedAccess}=await import('@erpx/server-commercial/public/purchase-command-boundary');expect(await readPurchasedAccess(env.H001_DB,'enforced',{companyId:f.companyId,capability:'people.payroll'})).toMatchObject({newWork:false,history:true});expect(await f.entry(target)).toMatchObject({purchasePurpose:'history'});

 for(const packageId of ['people','people-payroll']){const prior=(await readCompanyPackageGrants(env.H001_DB,f.companyId)).find(grant=>grant.packageId===packageId)!;await f.store.apply({expectedVersion:prior.version,idempotencyKey:key(),grant:{...prior,version:prior.version+1,state:'draining',reason:'Retain original settlement and owner history',evidence:'test:financial-entry-draining'}});expect(await f.entry(target)).toMatchObject({purchasePurpose:'history'});}
 const child=(await readCompanyPackageGrants(env.H001_DB,f.companyId)).find(grant=>grant.packageId==='people-payroll')!;
 // Exact native run remains a real duty, so withdrawal may not invent zero.
 await expect(f.withOwnerPackageObligations(env.H001_DB,{companyId:f.companyId,packageId:child.packageId,packageVersion:1},async transaction=>f.store.batch.applyIn(transaction,{companyId:f.companyId,expectedRevision:f.packageGrantRevision(f.companyId,await readCompanyPackageGrants(transaction,f.companyId)),idempotencyKey:key(),changes:[{expectedVersion:child.version,grant:{...child,version:child.version+1,state:'inactive',reason:'Attempt with retained native run',evidence:'test:real-duty-not-zero'}}]},async()=>{}))).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 expect((await db.prepare('SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id').bind(f.companyId).all()).results).toEqual(image);expect(await readWorkSelection(env.H001_DB,f.owner.sessionId,f.owner.userId,f.companyId)).toEqual(current);
 const originalContext=await readWorkSelection(env.H001_DB,f.owner.sessionId,f.owner.userId,f.companyId);
 const restored=await selectWorkRole(env.H001_DB,{sessionId:f.owner.sessionId,userId:f.owner.userId,companyId:f.companyId,assignmentId:f.owner.assignmentId,assignmentVersion:1,revision:originalContext!.revision,expiresAt:now()+600000});
 const restoredVersion=restored.assignment_version;if(typeof restoredVersion!=='number'||!Number.isSafeInteger(restoredVersion)||restoredVersion<1)throw Error('CURRENT_RESTORED_ASSIGNMENT_VERSION_REQUIRED');
 f.original.authority={sessionId:f.owner.sessionId,assignmentVersion:restoredVersion,revision:restored.revision};f.original.iam=await loadIamAuthority(db,f.companyId,f.owner.userId,f.owner.role,f.owner.sessionId);
 const {cancelPayrollRun}=await import('@erpx/server-people_payroll/public/payroll-cancellation');
 const cancelled=await cancelPayrollRun(f.original,String(f.run.id),{expectedVersion:1,reason:'Cancel unused native entry-only payroll preparation'},key());expect(cancelled.status).toBe('cancelled');
 await f.withOwnerPackageObligations(env.H001_DB,{companyId:f.companyId,packageId:child.packageId,packageVersion:1},async transaction=>f.store.batch.applyIn(transaction,{companyId:f.companyId,expectedRevision:f.packageGrantRevision(f.companyId,await readCompanyPackageGrants(transaction,f.companyId)),idempotencyKey:key(),changes:[{expectedVersion:child.version,grant:{...child,version:child.version+1,state:'inactive',reason:'Withdraw after exact native cancellation',evidence:'test:financial-entry-retained-history'}}]},async()=>{}));
 expect(await f.entry(target)).toMatchObject({purchasePurpose:'history'});
 const oldMode=process.env.NODE_ENV;vi.stubEnv('NODE_ENV','production');try{expect(await f.entry(target)).toBeNull();}finally{vi.stubEnv('NODE_ENV',oldMode);}
});

it('preserves original Expense accounting preparation and independent review through native Finance-only custom roles without Treasury or People discovery',async()=>{
 const {expenseFixture}=await import('../../support/helpers/expense-fixture'),expenses=await import('@erpx/server-people_payroll/public/expenses'),{saveExpenseAccounting}=await import('@erpx/server-people_payroll/public/expense-workflow'),{packageGrantStore,readCompanyPurchaseFacts}=await import('@erpx/server-commercial/public/package-grants'),{ownerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),{peopleFinancialProductEntry}=await import('@erpx/server-people_payroll/public/financial-product-entry'),{accountWorkModuleAccess}=await import('@erpx/server-identity_access/public/work-module-access'),{createProductWorkspace}=await import('@erpx/server-identity_access/application/product-workspace'),{productWorkspaceTarget}=await import('@erpx/server-identity_access/domain/product-workspace'),{initializeWorkSelection,readWorkSelection}=await import('@erpx/server-work_projection/public/work-role-store');
 // Reuse the original Expense imported staff/Finance-reference/evidence fixture.
 // Only normal owner commands below create/change claims, routes or accounting.
 // Account/session/MFA rows are declared native test-authority setup, not a
 // normal login/enrolment claim and never an installed role or MFA operation.
 const e=await expenseFixture(),reviewer=await e.member('independent-accounting-IAM-reviewer','holding_owner',10,e.accountsDepartment),scope={kind:'holding' as const,holdingId:'h001' as const,holdingSlug:e.owner.holdingSlug,holdingName:e.owner.holdingName,companyId:e.company};
 async function nativeSession(base:WorkContext,admin=false){
  const sessionId=key();await env.CONTROL_DB.batch([
   env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?)').bind(base.userId,base.name,base.email,now(),now()),
   env.CONTROL_DB.prepare('INSERT INTO session(id,"expiresAt",token,"createdAt","updatedAt","userId") VALUES(?,?,?,?,?,?)').bind(sessionId,new Date(now()+86400000).toISOString(),key(),new Date().toISOString(),new Date().toISOString(),base.userId),
  ]);
  if(admin)await env.CONTROL_DB.batch([env.CONTROL_DB.prepare('INSERT INTO iam_mfa(user_id,secret,enabled) VALUES(?,?,true)').bind(base.userId,await sealSecret('JBSWY3DPEHPK3PXP')),env.CONTROL_DB.prepare('INSERT INTO iam_mfa_sessions VALUES(?,?,?)').bind(sessionId,base.userId,now())]);
  const assignment=(await workspaceGrantsForAccount(env.H001_DB,e.company,base.userId)).rows.find(row=>row.id===base.assignmentId);if(!assignment)throw Error('ORIGINAL_NATIVE_EXPENSE_ASSIGNMENT_REQUIRED');
  const selected=await selectWorkRole(db,{sessionId,userId:base.userId,companyId:e.company,assignmentId:assignment.id,assignmentVersion:assignment.version,revision:0,expiresAt:now()+86400000});
  const authority={sessionId,revision:selected.revision,assignmentVersion:assignment.version},iam=await loadIamAuthority(db,e.company,base.userId,base.role,sessionId);
  return {base,sessionId,admin:{...base,sessionId,scopeKind:'holding' as const,scopeNodeId:null,platform:false,authority,iam} satisfies IamActor};
 }
 const author=await nativeSession(e.owner,true),review=await nativeSession(reviewer,true),preparation=await nativeSession(e.reviewer),approval=await nativeSession(e.manager),store=packageGrantStore(env.H001_DB,{authorize:async(_database,companyId)=>{if(companyId!==e.company)throw Error('EXACT_EXPENSE_ACCOUNTING_FIXTURE_COMPANY');return {userId:'declared-native-expense-provider'};},obligations:(transaction,companyId,packageId)=>ownerPackageObligations(env.H001_DB,transaction,companyId,packageId)});
 for(const packageId of ['people','people-expenses'])await store.apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:e.company,packageId,packageVersion:1,state:'active',startsAt:now()-1000,endsAt:null,version:1,reason:'Original Expense parent and addon only',evidence:'test:original-expense-accounting-entry'}});
 const priorMode=process.env.ERPX_PEOPLE_PACKAGE_ENFORCEMENT;vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');try{
  async function publish(subject:typeof preparation,action:'prepare'|'approve'){
   const permissions=['module.finance.read','finance.read',action==='prepare'?'finance.write':'finance.approve','expense.read','expense.review.read',`expense.${action}`,`workflow.finance_${action==='prepare'?'preparer':'approver'}`,'records.read','records.write','iam.self'],role=await saveIamRole(db,author.admin,roleInput.parse({name:'Native Expense accounting '+action,expectedVersion:0,permissions,scopes:['holding'],reason:'Original Expense accounting responsibility without People or Treasury authority'}),key());
   await decideIamReview(db,review.admin,role.id,{expectedVersion:1,decision:'approve',reason:'Independent review of the exact original accounting permission set'},key());
   const proposal=await grantIamAccess(db,author.admin,grantInput.parse({userId:subject.base.userId,roleId:role.roleId,scopeKind:'holding',validUntil:now()+86400000,reason:'Explicit independent custom accounting assignment'}),key());await decideIamReview(db,review.admin,proposal.id,{expectedVersion:1,decision:'approve',reason:'Independent approval of the actual accounting assignment'},key());
   const assignment=(await workspaceGrantsForAccount(env.H001_DB,e.company,subject.base.userId)).rows.find(row=>row.role===role.roleId);if(!assignment||typeof assignment.version!=='number'||!Number.isSafeInteger(assignment.version)||assignment.version<1)throw Error('ACTUAL_PUBLISHED_ACCOUNTING_ASSIGNMENT_REQUIRED');
   const account=async()=>{const {rows,roles}=await workspaceGrantsForAccount(env.H001_DB,e.company,subject.base.userId);return {scope,userId:subject.base.userId,sessionId:subject.sessionId,expiresAt:now()+600000,name:subject.base.name,email:subject.base.email,requestHeaders:new Headers(),assignments:rows.filter(row=>roles.get(row.role)?.scopes.includes(row.scope_kind)).map(row=>({id:row.id,role:row.role,companyId:row.company_id,branchId:row.branch_id,scopeKind:row.scope_kind,scopeNodeId:row.scope_node_id,modules:JSON.parse(row.modules_json)as string[],version:row.version}))};};
   const current=await account(),native=await accountWorkModuleAccess(current);expect(native.modulesByAssignment.get(assignment.id)).toEqual(['finance']);expect(native.permissionsByAssignment.get(assignment.id)).toEqual(expect.arrayContaining(['expense.read',`expense.${action}`]));for(const permission of ['expense.pay','expense.audit.read','module.people_payroll.read'])expect(native.permissionsByAssignment.get(assignment.id)).not.toContain(permission);
   const sourceScope={companyId:e.company,branchId:null,departmentId:null,userId:subject.base.userId,assignmentId:assignment.id,assignmentVersion:assignment.version,role:role.roleId,sessionId:subject.sessionId};
   const service=createProductWorkspace({access:account,modules:accountWorkModuleAccess,purchaseFacts:account=>readCompanyPurchaseFacts(env.H001_DB,account.scope.companyId),ready:async()=>true,peopleFinancialEntry:(account,choice,target)=>peopleFinancialProductEntry(env.H001_DB,{...sourceScope,assignmentId:choice.id,assignmentVersion:choice.version!,role:choice.role},target),selection:(account,eligible)=>initializeWorkSelection(env.H001_DB,{sessionId:account.sessionId,userId:account.userId,companyId:e.company,expiresAt:account.expiresAt},account.assignments,eligible),select:(account,option,revision)=>selectWorkRole(env.H001_DB,{sessionId:account.sessionId,userId:account.userId,companyId:e.company,expiresAt:account.expiresAt,assignmentId:option.assignmentId,assignmentVersion:option.assignmentVersion,revision}),now,error:(status,code,message)=>new(class extends Error{constructor(){super(message);Object.assign(this,{status,code});}})()});
   const actor=async():Promise<WorkContext>=>{const selected=await readWorkSelection(env.H001_DB,subject.sessionId,subject.base.userId,e.company);if(selected?.assignment_id!==assignment.id||selected.assignment_version!==assignment.version)throw Error('EXPLICIT_CURRENT_ACCOUNTING_ROLE_REQUIRED');const native=(await accountWorkModuleAccess(await account())).modulesByAssignment.get(assignment.id);if(!native||native.length!==1||native[0]!=='finance')throw Error('CURRENT_NATIVE_FINANCE_CEILING_REQUIRED');return {...subject.base,role:role.roleId,assignmentId:assignment.id,modules:[...native],authority:{sessionId:subject.sessionId,revision:selected.revision,assignmentVersion:assignment.version},iam:await loadIamAuthority(env.H001_DB,e.company,subject.base.userId,role.roleId,subject.sessionId)};};
   return {sourceScope,service,assignment,account,actor};
  }
  const preparer=await publish(preparation,'prepare'),approver=await publish(approval,'approve'),assignments=()=>db.prepare('SELECT id,user_id,role,version,modules_json,revoked_at FROM assignments WHERE company_id=? ORDER BY id').bind(e.company).all().then(result=>result.results),beforeAssignments=await assignments(),beforePurchases=await readCompanyPurchaseFacts(env.H001_DB,e.company),id=await e.claim(),hidden=await e.claim(e.head);await e.attach(id);await e.action(e.claimant,id,'submit');await e.action(e.head,id,'approve');
  // The private/unrouted claimant draft is hidden by the original native
  // reader. No audit role is silently introduced to make either source pass.
  const privateId=hidden;for(const candidate of [preparer,approver])expect(await peopleFinancialProductEntry(env.H001_DB,candidate.sourceScope,{kind:'expense',id:privateId})).toBeNull();
  async function enter(candidate:typeof preparer){const target=`/holdings/${e.owner.holdingSlug}/finance/documents/expenses/${id}/overview?source=actual#accounting`,input=productWorkspaceTarget('finance',e.company,target)!;expect(await peopleFinancialProductEntry(env.H001_DB,candidate.sourceScope,{kind:'expense',id})).toMatchObject({purchasePurpose:'history'});const state=await candidate.service.read('finance',input);expect(state.options.some(option=>option.assignmentId===candidate.assignment.id&&option.sourceRecord)).toBe(true);const selection=await readWorkSelection(env.H001_DB,candidate.sourceScope.sessionId,candidate.sourceScope.userId,e.company);if(!selection)throw Error('ORIGINAL_ACCOUNTING_SELECTION_REQUIRED');await candidate.service.choose('finance',input,candidate.assignment.id,selection.revision,candidate.assignment.version);expect(await candidate.service.read('finance',input)).toMatchObject({target,sourceRecord:true});const actor=await candidate.actor();expect(actor.modules).toEqual(['finance']);await expect(expenses.getExpense(actor,privateId)).rejects.toMatchObject({status:404});for(const path of ['finance/documents','people-organization/people/directory/current']){const adjacent=productWorkspaceTarget(path.startsWith('finance')?'finance':'people',e.company,`/holdings/${e.owner.holdingSlug}/${path}`)!;const response=await candidate.service.read(path.startsWith('finance')?'finance':'people',adjacent).catch(()=>null);expect(response?.options.some(option=>option.assignmentId===candidate.assignment.id)??false).toBe(false);}return actor;}
  const accounting=await enter(preparer),row=await expenses.getExpense(accounting,id);expect(row).toMatchObject({workflow_stage:'accounts_review',can_decide:1});const body={expectedVersion:Number(row.version),categoryAccountId:e.accounts.expense,tax:'7',periodId:e.period,entryDate:'2026-09-02',note:'Actual original independent accounting verification'},command=key(),saved=await saveExpenseAccounting(accounting,id,body,command);expect(await saveExpenseAccounting(accounting,id,body,command)).toEqual(saved);const prepared=await expenses.getExpense(accounting,id);await expenses.transitionExpense(accounting,id,'approve',{expectedVersion:Number(prepared.version),reason:'Original Accounts review completes its retained preparation'},key());
  const financial=await enter(approver),toApprove=await expenses.getExpense(financial,id);expect(toApprove).toMatchObject({workflow_stage:'accounts_approval',can_decide:1});const approved=await expenses.transitionExpense(financial,id,'approve',{expectedVersion:Number(toApprove.version),reason:'Independent original accounting approver accepts the actual preparation'},key());expect(approved).toMatchObject({id});expect(await expenses.getExpense(financial,id)).toMatchObject({workflow_stage:'gl_posting',status:'approved'});expect(await assignments()).toEqual(beforeAssignments);expect(await readCompanyPurchaseFacts(env.H001_DB,e.company)).toEqual(beforePurchases);
 }finally{vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT',priorMode);}
});
