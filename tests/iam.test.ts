import { beforeAll, expect, it, vi } from "vitest";
import { env, applyMigrations } from "./runtime-env";
import { iamDb } from "../src/server/iam-db";
import { accessComparison, accessDetail, accessEntries, peopleAccessData } from "../src/server/people-access";
import { emptyDirectoryHierarchy, validateDirectoryHierarchy } from "../src/lib/hierarchy";
import { loadIamAuthority, iamRole } from "../src/server/iam-store";
import { iamCommand } from "../src/server/iam-transactions";
import {
  iamOverview,
  iamAuditHistory,
  saveIamRole,
  roleInput,
  decideIamReview,
  grantIamAccess,
  grantInput,
  revokeIamAssignment,
} from "../src/server/iam-service";
import {
  connectionInput,
  saveConnection,
  applyDirectorySnapshot,
  connectionById,
  proposeMapping,
  mappingInput,
  applyDirectoryReview,
} from "../src/server/iam-directories";
import {
  prepareMigration,
  migrationInput,
  applyMigration,
} from "../src/server/iam-migrations";
import { sealSecret, totp, verifyMfa } from "../src/server/iam-mfa";
import { adGuid, escapeLdap, ldapEnabled } from "../src/server/iam-providers";
import {
  permissionCatalog,
  hasPermission,
  evaluateAccess,
  builtInPermissions,
} from "../src/lib/permissions";
import { samePerson } from "../src/lib/iam";
import { executeCommand } from "../src/server/commands";
import { selectWorkRole } from "../src/server/work-role-store";
import type { IamActor } from "../src/server/iam-context";
vi.mock("../src/server/runtime", () => ({
  runtimeEnv: async () => env,
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
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

it("protects the parent authority of the final recovery administrator", async () => {
  const f = await fixture(),
    recovery = await f.user();
  await db
    .prepare("UPDATE assignments SET parent_id=?,valid_until=? WHERE id=?")
    .bind(f.owner.assignmentId, now() + 86400000, recovery.assignmentId)
    .run();
  await db
    .prepare(
      "INSERT INTO iam_members(company_id,user_id,principal_id,login_policy) VALUES(?,?,?,'emergency')",
    )
    .bind(f.companyId, recovery.userId, "local:" + recovery.userId)
    .run();
  await expect(
    db
      .prepare(
        "UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?",
      )
      .bind(now(), f.owner.assignmentId)
      .run(),
  ).rejects.toThrow();
  expect(
    (await db
      .prepare("SELECT iam_assignment_active(?,?) ok")
      .bind(recovery.assignmentId, now())
      .first())!.ok,
  ).toBe(true);
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
  const { inspectIamAccess } = await import("../src/server/iam-inspector");
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
