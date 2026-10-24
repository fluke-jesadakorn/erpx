import { beforeAll, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import { expenseFixture, expenseDb as db, key } from "../../support/helpers/expense-fixture";
import {installStaffLevels,positionSchema,savePosition} from '@erpx/server-organization/public/organization';
import { previewPositionMove, submitPositionMove, drainPositionMoves, cancelPositionMove, positionMoves } from "@erpx/server-organization/public/position-moves";
import { positionMoveDate, positionMoveInput, positionMoveSubmitInput, type PositionMoveRecord } from "@erpx/feature-people_payroll/lib/position-moves";
import { lifecycleInput } from "@erpx/feature-people_payroll/lib/people-lifecycle";
import { previewLifecycle, saveLifecycle } from "@erpx/server-people_payroll/public/people-lifecycle";
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: db, files: env.H001_FILES }), runtimeEnv: async () => env }));
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS); });

async function fixture() {
  const f = await expenseFixture(false), hr = await f.member("hr", "hr_admin");
  await installStaffLevels(db, f.owner, key());
  const level = (n: number) => `org-level-${f.company}-${n}`;
  const ceo = String((await savePosition(db, f.owner, null, positionSchema.parse({ title: "CEO", branchId: null, departmentId: null, levelId: level(1), reportsToPositionId: null, isCeo: true, expectedVersion: 0, reason: "Prepare fixture" }), key())).id);
  const position = String((await savePosition(db, hr, null, positionSchema.parse({ title: "Team member", branchId: f.branch, departmentId: f.department, levelId: level(6), reportsToPositionId: ceo, expectedVersion: 0, reason: "Prepare fixture" }), key())).id);
  await db.prepare("UPDATE hr_engagements SET position_id=?,level_id=?,manager_person_id=NULL WHERE person_id=?").bind(ceo, level(1), `person-${f.owner.userId}`).run();
  for (const [person, n] of [[f.claimant, 6], [f.tie, 5]] as const) await db.prepare("UPDATE hr_engagements SET position_id=?,level_id=?,manager_person_id=? WHERE person_id=?").bind(position, level(n), `person-${f.owner.userId}`, `person-${person.userId}`).run();
  const move = (extra = {}) => positionMoveInput.parse({ positionId: position, expectedVersion: 1, targetKind: "department", targetId: f.accountsDepartment, effectiveAt: Date.now() + 120_000, reason: "Move reviewed team together", ...extra });
  return { ...f, hr, position, ceo, level, move, first: `eng-${f.claimant.userId}`, second: `eng-${f.tie.userId}` };
}

it("moves all holders atomically while retaining differing Levels, statuses and employment dates", async () => {
  const f = await fixture();
  await db.prepare("UPDATE hr_engagements SET status='suspended',end_date='2030-12-31' WHERE id=?").bind(f.second).run();
  await db.prepare("UPDATE hr_people SET status='inactive' WHERE id=?").bind(`person-${f.tie.userId}`).run();
  const input = f.move(), preview = await previewPositionMove(db, f.hr, input);
  expect(preview.ready, preview.blockers.join(" ")).toBe(true); expect(preview.affectedCount).toBe(2);
  expect(preview.people.map(person => person.after.levelId).sort()).toEqual([f.level(5), f.level(6)].sort());
  const command = key(), submitted = await submitPositionMove(db, f.hr, input, preview.previewToken, preview.expiresAt, command);
  expect(await submitPositionMove(db, f.hr, input, preview.previewToken, preview.expiresAt, command)).toEqual(submitted);
  await drainPositionMoves(db, input.effectiveAt + 1);
  expect(await positionMoves(db, f.hr, String(submitted.id))).toMatchObject({ status: "applied", version: 2 });
  expect(await db.prepare("SELECT department_id,level_id,status,start_date,end_date FROM hr_engagements WHERE id=?").bind(f.second).first()).toEqual({ department_id: f.accountsDepartment, level_id: f.level(5), status: "suspended", start_date: "2020-01-01", end_date: "2030-12-31" });
  expect(await db.prepare("SELECT status FROM hr_people WHERE id=?").bind(`person-${f.tie.userId}`).first()).toEqual({ status: "inactive" });
  expect((await db.prepare("SELECT count(*) n FROM staff_organization_history WHERE command_id=?").bind(submitted.id).first<{ n: number }>())?.n).toBe(2);
  expect((await db.prepare("SELECT count(*) n FROM staff_level_history WHERE command_id=?").bind(submitted.id).first<{ n: number }>())?.n).toBe(0);
  await drainPositionMoves(db, input.effectiveAt + 2);
  expect((await db.prepare("SELECT count(*) n FROM staff_organization_history WHERE command_id=?").bind(submitted.id).first<{ n: number }>())?.n).toBe(2);
});

it("applies actual Level to all holders with level history and next-January leave, preserving custom leave and salary", async () => {
  const f = await fixture(), target = f.level(4), policy = key(), custom = key();
  for (const [id, policyKey] of [[policy, "level-4-default"], [custom, "custom-team-leave"]]) await db.prepare("INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,effective_from,created_by,created_at) VALUES(?,?,?,1,?,'active','2020-01-01',?,1)").bind(id, f.company, policyKey, policyKey, f.owner.userId).run();
  await db.prepare("INSERT INTO level_leave_defaults(id,company_id,level_id,version,policy_version_id,created_by,created_at) VALUES(?,?,?,1,?,?,1)").bind(key(), f.company, target, policy, f.owner.userId).run();
  await db.prepare("INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,effective_from,created_by,created_at) VALUES(?,?,?,?,'2020-01-01',?,1)").bind(key(), f.company, f.second, custom, f.owner.userId).run();
  const input = f.move({ targetKind: "level", targetId: target }), preview = await previewPositionMove(db, f.hr, input);
  expect(preview.ready, preview.blockers.join(" ")).toBe(true);
  const nextYear = `${Number(positionMoveDate(input.effectiveAt, input.timezone).slice(0, 4)) + 1}-01-01`;
  expect(preview.people.find(p => p.engagementId === f.first)?.leaveEffectiveFrom).toBe(nextYear);
  expect(preview.people.find(p => p.engagementId === f.second)?.leaveNote).toBe("Custom leave policy is preserved.");
  const payBefore = (await db.prepare("SELECT * FROM hr_compensation_revisions WHERE engagement_id IN (?,?) ORDER BY id").bind(f.first, f.second).all()).results;
  const submitted = await submitPositionMove(db, f.hr, input, preview.previewToken, preview.expiresAt, key());
  await drainPositionMoves(db, input.effectiveAt + 1);
  expect(await positionMoves(db, f.hr, String(submitted.id))).toMatchObject({ status: "applied" });
  expect((await db.prepare("SELECT level_id,department_id FROM hr_engagements WHERE id IN (?,?)").bind(f.first, f.second).all()).results).toEqual([{ level_id: target, department_id: f.department }, { level_id: target, department_id: f.department }]);
  expect((await db.prepare("SELECT count(*) n FROM staff_level_history WHERE command_id=?").bind(submitted.id).first<{ n: number }>())?.n).toBe(2);
  expect(await db.prepare("SELECT policy_version_id,effective_from FROM leave_policy_assignments WHERE engagement_id=?").bind(f.first).first()).toEqual({ policy_version_id: policy, effective_from: nextYear });
  expect((await db.prepare("SELECT * FROM hr_compensation_revisions WHERE engagement_id IN (?,?) ORDER BY id").bind(f.first, f.second).all()).results).toEqual(payBefore);
  expect(await db.prepare("SELECT policy_version_id FROM leave_policy_assignments WHERE engagement_id=?").bind(f.second).first()).toEqual({ policy_version_id: custom });
});

it("rejects expired or edited previews and stops the entire move on stale cohort at effective time", async () => {
  const f = await fixture(), input = f.move(), preview = await previewPositionMove(db, f.hr, input);
  await expect(submitPositionMove(db, f.hr, input, preview.previewToken, Date.now() - 1, key())).rejects.toMatchObject({ code: "POSITION_MOVE_PREVIEW_EXPIRED" });
  await expect(submitPositionMove(db, f.hr, { ...input, reason: "Changed reviewed reason" }, preview.previewToken, preview.expiresAt, key())).rejects.toMatchObject({ code: "POSITION_MOVE_CHANGED" });
  const submitted = await submitPositionMove(db, f.hr, input, preview.previewToken, preview.expiresAt, key());
  await db.prepare("UPDATE hr_engagements SET status='ended',version=version+1 WHERE id=?").bind(f.second).run();
  await drainPositionMoves(db, input.effectiveAt + 1);
  expect(await positionMoves(db, f.hr, String(submitted.id))).toMatchObject({ status: "needs_review", last_error: expect.stringContaining("POSITION_MOVE_REVIEW_CHANGED") });
  expect(await db.prepare("SELECT department_id FROM hr_engagements WHERE id=?").bind(f.first).first()).toEqual({ department_id: f.department });
  expect(await db.prepare("SELECT department_id,version FROM organization_positions WHERE id=?").bind(f.position).first()).toEqual({ department_id: f.department, version: 1 });
});

it("cancels before effective time and applies nothing after cancellation", async () => {
  const f = await fixture(), input = f.move(), preview = await previewPositionMove(db, f.hr, input);
  const submitted = await submitPositionMove(db, f.hr, input, preview.previewToken, preview.expiresAt, key());
  await cancelPositionMove(db, f.hr, String(submitted.id), 1, key());
  await drainPositionMoves(db, input.effectiveAt + 1);
  expect(await positionMoves(db, f.hr, String(submitted.id))).toMatchObject({ status: "cancelled" });
  expect(await db.prepare("SELECT department_id FROM organization_positions WHERE id=?").bind(f.position).first()).toEqual({ department_id: f.department });
});

it("rechecks authorizing permission at effective time", async () => {
  const f = await fixture(), input = f.move(), preview = await previewPositionMove(db, f.hr, input);
  const submitted = await submitPositionMove(db, f.hr, input, preview.previewToken, preview.expiresAt, key());
  await db.prepare("UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?").bind(Date.now(), f.hr.assignmentId).run();
  await drainPositionMoves(db, input.effectiveAt + 1);
  expect(await positionMoves(db, f.owner, String(submitted.id))).toMatchObject({ status: "needs_review", last_error: expect.stringContaining("POSITION_MOVE_AUTHORITY_CHANGED") });
  expect(await db.prepare("SELECT department_id FROM hr_engagements WHERE id=?").bind(f.first).first()).toEqual({ department_id: f.department });
});

it("validates reporting positions, employee managers, CEO and external direct reports", async () => {
  const f = await fixture();
  const selfManager = await previewPositionMove(db, f.hr, f.move({ managerAssignments: [{ engagementId: f.first, managerPersonId: `person-${f.claimant.userId}` }] }));
  expect(selfManager.ready).toBe(false); expect(selfManager.blockers.join(" ")).toMatch(/circular/);
  const ceo = await previewPositionMove(db, f.hr, f.move({ positionId: f.ceo, targetKind: "level", targetId: f.level(2) }));
  expect(ceo.ready).toBe(false); expect(ceo.blockers.join(" ")).toContain("CEO position");
  const report = String((await savePosition(db, f.hr, null, positionSchema.parse({ title: "Junior report", branchId: f.branch, departmentId: f.department, levelId: f.level(6), reportsToPositionId: f.ceo, expectedVersion: 0, reason: "Add report fixture" }), key())).id);
  await db.prepare("UPDATE hr_engagements SET position_id=?,level_id=?,manager_person_id=? WHERE id=?").bind(report, f.level(6), `person-${f.tie.userId}`, `eng-${f.head.userId}`).run();
  const blocked = await previewPositionMove(db, f.hr, f.move({ targetKind: "level", targetId: f.level(6) }));
  expect(blocked.ready).toBe(false); expect(blocked.blockers.join(" ")).toContain("more senior level");
  const invalidParent = await previewPositionMove(db, f.hr, f.move({ targetKind: "level", targetId: f.level(4), reportsToPositionId: report }));
  expect(invalidParent.ready).toBe(false); expect(invalidParent.blockers.join(" ")).toContain("reporting position must have a more senior");
});

it("prevents overlapping whole-position requests and individual placements in either order", async () => {
  const f = await fixture(), input = f.move(), preview = await previewPositionMove(db, f.hr, input);
  const submitted = await submitPositionMove(db, f.hr, input, preview.previewToken, preview.expiresAt, key());
  expect((await previewPositionMove(db, f.hr, input)).blockers.join(" ")).toContain("already has an open placement");
  const individual = lifecycleInput.parse({ kind: "transfer", expectedVersion: 0, personId: `person-${f.claimant.userId}`, engagementId: f.first, positionId: f.position, managerPersonId: `person-${f.owner.userId}`, effectiveAt: Date.now() + 180_000, reason: "Individual transfer" });
  const placementPreview = await previewLifecycle(db, f.hr, individual, true);
  await expect(saveLifecycle(db, f.hr, individual, key(), true, placementPreview.token, true)).rejects.toThrow();
  await cancelPositionMove(db, f.hr, String(submitted.id), 1, key());
  const fresh = await previewLifecycle(db, f.hr, individual, true);
  await saveLifecycle(db, f.hr, individual, key(), true, fresh.token, true);
  expect((await previewPositionMove(db, f.hr, input)).ready).toBe(false);
});

it("requires both organization and people authority and validates manager inputs", async () => {
  const f = await fixture();
  await expect(previewPositionMove(db, f.claimant, f.move())).rejects.toMatchObject({ code: "PERMISSION_REQUIRED" });
  const wrongScope = { ...f.hr, branchId: f.branch, departmentId: f.department };
  await expect(previewPositionMove(db, wrongScope, f.move())).rejects.toMatchObject({ code: "SCOPE_FORBIDDEN" });
  expect(positionMoveInput.safeParse({ ...f.move(), managerAssignments: [{ engagementId: f.first, managerPersonId: null }, { engagementId: f.first, managerPersonId: null }] }).success).toBe(false);
  expect(positionMoveSubmitInput.safeParse({ ...f.move(), previewToken: "token", expiresAt: Date.now() }).success).toBe(true);
  const invalidMember = await previewPositionMove(db, f.hr, f.move({ managerAssignments: [{ engagementId: "unrelated", managerPersonId: null }] }));
  expect(invalidMember.ready).toBe(false);
});

it("rolls back every employee and the position when a write fails mid-batch", async () => {
  const f = await fixture(), input = f.move(), preview = await previewPositionMove(db, f.hr, input);
  const submitted = await submitPositionMove(db, f.hr, input, preview.previewToken, preview.expiresAt, key());
  // A collision on the second history write forces a failure after the first employee was updated.
  const last = preview.people.at(-1)!;
  await db.prepare("INSERT INTO staff_organization_history(id,company_id,engagement_id,before_json,after_json,actor_id,reason,command_id,created_at) VALUES(?,?,?,'{}','{}',?,'Collision fixture',?,1)")
    .bind(`position-move-history-${submitted.id}-${last.engagementId}`, f.company, last.engagementId, f.owner.userId, key()).run();
  await drainPositionMoves(db, input.effectiveAt + 1);
  const status = await positionMoves(db, f.hr, String(submitted.id)) as PositionMoveRecord;
  expect(status.status).toBe("needs_review");
  expect((await db.prepare("SELECT department_id FROM hr_engagements WHERE id IN (?,?)").bind(f.first, f.second).all()).results).toEqual([{ department_id: f.department }, { department_id: f.department }]);
  expect(await db.prepare("SELECT department_id,version FROM organization_positions WHERE id=?").bind(f.position).first()).toEqual({ department_id: f.department, version: 1 });
});

it("retains direct Position GPOs across a move, then stops if reviewed policy semantics change", async () => {
  const f = await fixture(), connection = key();
  await db.prepare("INSERT INTO iam_connections(id,company_id,name,provider,config,status,created_by,created_at) VALUES(?,?,'Move preview','active_directory','{}'::jsonb,'active',?,1)").bind(connection, f.company, f.owner.userId).run();
  const rules = [{ id: key(), kind: "position", target: f.position, gpo: "shared" }];
  for (const rule of rules) await db.prepare("INSERT INTO organization_gpo_rules(id,company_id,target_kind,target_id,version,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,last_actor_id,last_reason,updated_at) VALUES(?,?,?,?,1,1,?,?,?,'computers',?,'Fixture reviewed policy',1)")
    .bind(rule.id, f.company, rule.kind, rule.target, connection, rule.gpo, `group-${rule.gpo}`, f.owner.userId).run();
  const input = f.move(), preview = await previewPositionMove(db, f.hr, input);
  expect(preview.policyChanges.added).toEqual([]);
  expect(preview.policyChanges.removed).toEqual([]);
  expect(preview.policyChanges.retained).toHaveLength(1);
  expect(preview.policyChanges.retained[0].beforeSources).toHaveLength(1);
  expect(preview.policyChanges.retained[0].afterSources).toHaveLength(1);
  const submitted = await submitPositionMove(db, f.hr, input, preview.previewToken, preview.expiresAt, key());
  await db.prepare("UPDATE organization_gpo_rules SET version=version+1,revision=revision+1,gpo_id='changed-policy' WHERE id=?").bind(rules[0].id).run();
  await drainPositionMoves(db, input.effectiveAt + 1);
  expect(await positionMoves(db, f.hr, String(submitted.id))).toMatchObject({ status: "needs_review" });
  expect(await db.prepare("SELECT department_id FROM organization_positions WHERE id=?").bind(f.position).first()).toEqual({ department_id: f.department });
  expect((await db.prepare("SELECT count(*) n FROM position_gpo_jobs WHERE company_id=?").bind(f.company).first<{ n: number }>())?.n).toBe(0);
});

it("allows HR to schedule a Position move while its GPO configuration is blocked", async () => {
  const f=await fixture(),connection=key(),rule=key();
  await db.prepare("INSERT INTO iam_connections(id,company_id,name,provider,config,status,created_by,created_at) VALUES(?,?,'Blocked GPO','active_directory','{}'::jsonb,'active',?,1)").bind(connection,f.company,f.owner.userId).run();
  await db.prepare(`INSERT INTO organization_gpo_rules(id,company_id,target_kind,target_id,version,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,directory_config_status,last_actor_id,last_reason,updated_at)
    VALUES(?,?,'position',?,1,1,?,'gpo','group','ou','blocked',?,'Provider unavailable',1)`)
    .bind(rule,f.company,f.position,connection,f.owner.userId).run();
  const input=f.move(),preview=await previewPositionMove(db,f.hr,input);
  expect(preview.ready).toBe(true);
  expect((await submitPositionMove(db,f.hr,input,preview.previewToken,preview.expiresAt,key())).status).toBe("scheduled");
});

it('rolls back People placement and audit on the final outbox failure, then permits a fresh reviewed move and replay',async()=>{
 const f=await fixture(),input=f.move(),preview=await previewPositionMove(db,f.hr,input),command=key(),submitted=await submitPositionMove(db,f.hr,input,preview.previewToken,preview.expiresAt,command);
 const before=(await db.prepare('SELECT id,version,branch_id,department_id,level_id,manager_person_id FROM hr_engagements WHERE id IN (?,?) ORDER BY id').bind(f.first,f.second).all()).results;
 const recordId=submitted.id.replaceAll("'","''");
 await db.exec(`CREATE FUNCTION fail_position_move_event_test() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.aggregate_id='${recordId}' AND NEW.event_type='organization.position-move.applied' THEN RAISE EXCEPTION 'forced final move event failure' USING ERRCODE='23514'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_position_move_event_test BEFORE INSERT ON outbox_events FOR EACH ROW EXECUTE FUNCTION fail_position_move_event_test()`);
 try{await drainPositionMoves(db,input.effectiveAt+1);}finally{await db.exec('DROP TRIGGER fail_position_move_event_test ON outbox_events; DROP FUNCTION fail_position_move_event_test()');}
 expect((await positionMoves(db,f.hr,submitted.id) as PositionMoveRecord).status).toBe('needs_review');
 expect((await db.prepare('SELECT id,version,branch_id,department_id,level_id,manager_person_id FROM hr_engagements WHERE id IN (?,?) ORDER BY id').bind(f.first,f.second).all()).results).toEqual(before);
 expect(await db.prepare('SELECT department_id,version FROM organization_positions WHERE id=?').bind(f.position).first()).toEqual({department_id:f.department,version:1});
 for(const table of ['staff_organization_history','audit_events','outbox_events'])expect((await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(submitted.id).first())?.n).toBe(0);
 const nextPreview=await previewPositionMove(db,f.hr,input),nextKey=key(),retried=await submitPositionMove(db,f.hr,input,nextPreview.previewToken,nextPreview.expiresAt,nextKey);
 expect(await submitPositionMove(db,f.hr,input,nextPreview.previewToken,nextPreview.expiresAt,nextKey)).toEqual(retried);
 await drainPositionMoves(db,input.effectiveAt+2);expect((await positionMoves(db,f.hr,retried.id) as PositionMoveRecord).status).toBe('applied');await drainPositionMoves(db,input.effectiveAt+3);
 expect((await db.prepare('SELECT department_id FROM hr_engagements WHERE id IN (?,?)').bind(f.first,f.second).all()).results).toEqual([{department_id:f.accountsDepartment},{department_id:f.accountsDepartment}]);
 for(const table of ['audit_events','outbox_events'])expect((await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(retried.id).first())?.n).toBe(1);
});
