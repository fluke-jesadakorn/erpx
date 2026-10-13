import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { projectSchema, saveProject } from "../../../src/server/modules/operations/public/staff-tasks";
import { listProjectMilestones, projectMilestoneSchema, saveProjectMilestone, transitionProjectMilestone } from "../../../src/server/modules/projects/public/project-milestones";
import {
  createProjectCostAllocation,
  decideProjectCostAllocation,
  decideProjectWorkLog,
  listProjectCostAllocations,
  listProjectCostSources,
  listProjectWorkLogs,
  projectCostAllocationDecisionSchema,
  projectCostAllocationSchema,
  projectWorkLogDecisionSchema,
  projectWorkLogSchema,
  saveProjectWorkLog,
} from "../../../src/server/modules/projects/public/project-actuals";
import { queryMetric } from "../../../src/server/modules/runtime/public/executive/metrics";
import { readExecutiveEvidence, searchExecutiveEvidence } from "../../../src/server/modules/runtime/public/executive/evidence";
import { createReport, saveReport, saveShare, saveSchema } from "../../../src/server/modules/runtime/public/executive/reports";
import { defaultFilters, definitionSchema, emptyDefinition, shareSchema } from "../../../src/lib/shared/executive/canvas";
import { listAttachmentSets,prepareAttachmentBinding } from "../../../src/server/modules/files/public/attachments/service";
import { attachmentPolicy,authorizeAttachmentTarget } from "../../../src/server/modules/files/public/attachments/policy";
import { attachmentBindingState,enableAttachmentV2ForTest,seedAttachmentSet } from "../../support/helpers/attachment-v2";

vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env }));

let f: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async () => { enableAttachmentV2ForTest();await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => { f = await expenseFixture(); });

it("keeps project documents inside exact project membership and owner authority",async()=>{
  const project=await saveProject(db,f.owner,null,projectSchema.parse({title:"Attachment ownership fixture",memberUserIds:[f.claimant.userId]}),key());
  const files=await seedAttachmentSet(db,f.owner,{targetType:"project",targetId:project.id,componentKind:"record",purposeKey:"project_document",expectedTargetVersion:1});
  expect(await listAttachmentSets(db,f.owner,{targetType:"project",targetId:project.id,purposeKey:"project_document",componentKind:"record"})).toHaveLength(1);
  const member={...f.claimant,role:"holding_owner",branchId:null,departmentId:null};member.assignmentId=await f.grant(member,"holding_owner");
  await expect(prepareAttachmentBinding(db,member,{attachmentSetIds:[files.setId],bindingContract:{targetType:"project",bindingType:"record_document",contexts:[{purposeKey:"project_document",componentKind:"record",componentRef:null}]},targetId:project.id,expectedTargetVersion:1,commandId:key(),bindingId:project.id})).rejects.toMatchObject({code:"ATTACHMENT_PROJECT_OWNER_REQUIRED"});
  expect(await attachmentBindingState(db,files.setId)).toMatchObject({set_state:"draft",link_state:"draft"});
  const outsider={...f.head,role:"holding_owner",branchId:null,departmentId:null};outsider.assignmentId=await f.grant(outsider,"holding_owner");
  await expect(authorizeAttachmentTarget(db,outsider,attachmentPolicy("project","project_document","record"),project.id,"read")).rejects.toMatchObject({status:404});
});

it("counts only independently approved project work logs and limits record evidence to the owner or contributor", async () => {
  const project = await saveProject(db, f.owner, null, projectSchema.parse({ title: "Executive actual-hours fixture", memberUserIds: [f.claimant.userId] }), key());
  const submitted = await saveProjectWorkLog(db, f.claimant, project.id, projectWorkLogSchema.parse({ workDate: "2026-09-12", minutes: 90 }), key());
  expect(submitted).toMatchObject({ state: "submitted", version: 1 });
  expect(await listProjectWorkLogs(db, f.claimant, project.id)).toMatchObject([{ is_actor: true, can_review: false }]);
  expect(await listProjectWorkLogs(db, f.owner, project.id)).toMatchObject([{ is_actor: false, can_review: true }]);
  await expect(decideProjectWorkLog(db, f.claimant, project.id, submitted.id, projectWorkLogDecisionSchema.parse({ action: "approve", expectedVersion: 1, reason: "Self-approval is not allowed." }), key()))
    .rejects.toMatchObject({ code: "PROJECT_OWNER_REQUIRED" });
  const rejected = await decideProjectWorkLog(db, f.owner, project.id, submitted.id, projectWorkLogDecisionSchema.parse({ action: "reject", expectedVersion: 1, reason: "Correct the time and resubmit it." }), key());
  expect(rejected).toMatchObject({ state: "rejected", version: 2 });
  const revised = await saveProjectWorkLog(db, f.claimant, project.id, projectWorkLogSchema.parse({ workDate: "2026-09-12", minutes: 125, expectedVersion: 2 }), key());
  expect(revised).toMatchObject({ state: "submitted", version: 3 });
  const approved = await decideProjectWorkLog(db, f.owner, project.id, submitted.id, projectWorkLogDecisionSchema.parse({ action: "approve", expectedVersion: 3, reason: "Corrected time approved by the project owner." }), key());
  expect(approved).toMatchObject({ state: "approved", version: 4 });

  const actor = { ...f.owner, modules: ["projects"] }, filters = { from: "2026-09-01", to: "2026-09-30", currency: "THB" };
  expect(await queryMetric(db, actor, "project-actual-hours", filters)).toMatchObject({ total: "7500000", unit: "duration", status: "ready" });
  expect((await queryMetric(db, actor, "project-actual-hours", filters, "month")).rows).toEqual([{ label: "2026-09", value: "7500000" }]);
  const search = await searchExecutiveEvidence(db, actor, { metric: "project-actual-hours", filters, query: "Executive actual-hours", limit: 10 });
  expect(search).toMatchObject({ provider: "projects-approved-actuals", metric: "project-actual-hours" });
  expect(search.items).toHaveLength(1);
  const detail = await readExecutiveEvidence(db, actor, "project-actual-hours", search.items[0].id, filters);
  expect(detail.record).toMatchObject({ project_id: project.id, work_date: "2026-09-12", minutes: 125, version: 4 });
  expect(detail.fields).not.toContain("rejection_reason");

  const memberSearch = await searchExecutiveEvidence(db, { ...f.claimant, modules: ["projects"] }, { metric: "project-actual-hours", filters, query: "Executive actual-hours", limit: 10 });
  expect(memberSearch.items.map((item) => item.id)).toEqual([submitted.id]);
  await db.prepare("DELETE FROM staff_project_members WHERE project_id=? AND user_id=?").bind(project.id, f.claimant.userId).run();
  await expect(readExecutiveEvidence(db, { ...f.claimant, modules: ["projects"] }, "project-actual-hours", submitted.id, filters))
    .rejects.toMatchObject({ code: "EVIDENCE_NOT_FOUND" });

  for (const metric of ["project-actual-hours", "project-posted-cost", "project-dependency-blockers"] as const) {
    const report = await createReport(db, actor, key(), `Project ${metric} distribution guard`);
    const definition = definitionSchema.parse({ ...emptyDefinition(`Project ${metric} distribution guard`), widgets: [
      { id: "project-metric", title: "Project actual", metric, kind: "kpi", width: "half" },
    ] });
    const filters = defaultFilters();
    await saveReport(db, actor, report.id, saveSchema.parse({ expectedVersion: 1, definition, filters }), key());
    await expect(saveShare(db, actor, report.id, shareSchema.parse({ recipientId: f.claimant.userId, mode: "distribution", widgetIds: ["project-metric"], expiresAt: null, filters }), key()))
      .rejects.toMatchObject({ code: "PERSONAL_METRIC" });
  }
});

it("orders project milestones by same-project prerequisites and exposes current blocker evidence", async () => {
  const project = await saveProject(db, f.owner, null, projectSchema.parse({ title: "Dependency workflow fixture", memberUserIds: [f.claimant.userId] }), key());
  const prerequisite = await saveProjectMilestone(db, f.owner, project.id, null, projectMilestoneSchema.parse({
    title: "Approve the engineering sample", dueAt: "2026-09-15T12:00:00.000Z", ownerUserId: f.owner.userId,
  }), key());
  const successor = await saveProjectMilestone(db, f.owner, project.id, null, projectMilestoneSchema.parse({
    title: "Begin the production trial", dueAt: "2026-09-20T12:00:00.000Z", ownerUserId: f.claimant.userId,
    dependsOnMilestoneIds: [prerequisite.id],
  }), key());
  const listed = await listProjectMilestones(db, f.claimant, project.id);
  expect(listed.find((row) => row.id === successor.id)).toMatchObject({
    depends_on_ids: [prerequisite.id], depends_on_titles: ["Approve the engineering sample"],
    blocked_by_titles: ["Approve the engineering sample"], blocked_by_count: 1,
  });

  await expect(transitionProjectMilestone(db, f.owner, project.id, successor.id, {
    action: "start", expectedVersion: 1, reason: "Start the dependent milestone.",
  }, key())).rejects.toMatchObject({ code: "PROJECT_DEPENDENCIES_INCOMPLETE" });
  await expect(transitionProjectMilestone(db, f.owner, project.id, prerequisite.id, {
    action: "cancel", expectedVersion: 1, reason: "Cancel a prerequisite with an active dependent.",
  }, key())).rejects.toMatchObject({ code: "PROJECT_MILESTONE_REQUIRED_PREREQUISITE" });
  await expect(saveProjectMilestone(db, f.owner, project.id, prerequisite.id, projectMilestoneSchema.parse({
    title: "Approve the engineering sample", dueAt: "2026-09-15T12:00:00.000Z", ownerUserId: f.owner.userId,
    expectedVersion: 1, dependsOnMilestoneIds: [successor.id],
  }), key())).rejects.toMatchObject({ code: "PROJECT_DEPENDENCY_CYCLE" });

  const actor = { ...f.owner, modules: ["projects"] }, filters = { from: "2026-09-01", to: "2026-09-30", currency: "THB" };
  expect(await queryMetric(db, actor, "project-dependency-blockers", filters)).toMatchObject({ total: "1", unit: "count", status: "ready" });
  expect((await queryMetric(db, actor, "project-dependency-blockers", filters, "project")).rows)
    .toEqual([{ label: "Dependency workflow fixture", value: "1" }]);
  const evidence = await searchExecutiveEvidence(db, actor, {
    metric: "project-dependency-blockers", filters, query: "engineering sample", limit: 10,
  });
  expect(evidence).toMatchObject({ provider: "projects-milestone-dependencies", metric: "project-dependency-blockers" });
  expect(evidence.items).toMatchObject([{ id: successor.id, summary: "Dependency workflow fixture · 1 incomplete prerequisite" }]);
  const detail = await readExecutiveEvidence(db, actor, "project-dependency-blockers", successor.id, filters);
  expect(detail.record).toMatchObject({
    id: successor.id, prerequisites: [{ id: prerequisite.id, title: "Approve the engineering sample", state: "planned", version: 1 }],
  });

  const memberActor = { ...f.claimant, modules: ["projects"] };
  await db.prepare("DELETE FROM staff_project_members WHERE project_id=? AND user_id=?").bind(project.id, f.claimant.userId).run();
  expect((await searchExecutiveEvidence(db, memberActor, { metric: "project-dependency-blockers", filters, limit: 10 })).items).toHaveLength(0);
  await expect(readExecutiveEvidence(db, memberActor, "project-dependency-blockers", successor.id, filters))
    .rejects.toMatchObject({ code: "EVIDENCE_NOT_FOUND" });

  await transitionProjectMilestone(db, f.owner, project.id, prerequisite.id, {
    action: "complete", expectedVersion: 1, reason: "Sample approval is complete.",
  }, key());
  expect((await listProjectMilestones(db, f.owner, project.id)).find((row) => row.id === successor.id))
    .toMatchObject({ blocked_by_titles: [], blocked_by_count: 0 });
  expect(await queryMetric(db, actor, "project-dependency-blockers", filters)).toMatchObject({ total: "0", status: "empty" });
  await expect(readExecutiveEvidence(db, actor, "project-dependency-blockers", successor.id, filters))
    .rejects.toMatchObject({ code: "EVIDENCE_NOT_FOUND" });
  await transitionProjectMilestone(db, f.owner, project.id, successor.id, {
    action: "start", expectedVersion: 1, reason: "Start after prerequisite approval.",
  }, key());
  await transitionProjectMilestone(db, f.owner, project.id, successor.id, {
    action: "complete", expectedVersion: 2, reason: "Complete the production trial.",
  }, key());
  await expect(transitionProjectMilestone(db, f.owner, project.id, prerequisite.id, {
    action: "reopen", expectedVersion: 2, reason: "Reopen a prerequisite with a completed dependent.",
  }, key())).rejects.toMatchObject({ code: "PROJECT_MILESTONE_DEPENDENT_COMPLETED" });
});

it("allocates only posted expense debits, requires an independent Finance approver, and excludes reversed source journals", async () => {
  const project = await saveProject(db, f.owner, null, projectSchema.parse({ title: "Executive actual-cost fixture", memberUserIds: [f.reviewer.userId, f.manager.userId] }), key());
  const journalId = key(), expenseLineId = key(), payableLineId = key(), now = Date.now();
  await db.batch([
    db.prepare(`INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,status,version,created_by,posted_by,posted_at,created_at,updated_at)
      VALUES(?,?,?,?,?,'2026-09-12','THB','manual','posted',1,?,?,?, ?,?)`).bind(journalId, f.company, f.chart, f.period, `PRJ-${journalId.slice(0,8)}`, f.owner.userId, f.owner.userId, now, now, now),
    db.prepare(`INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal)
      VALUES(?,?,?,10000,0,10000,0,'{}',0),(?,?,?,0,10000,0,10000,'{}',1)`)
      .bind(expenseLineId, journalId, f.accounts.expense, payableLineId, journalId, f.accounts.payable),
  ]);

  const prepared = await createProjectCostAllocation(f.owner, projectCostAllocationSchema.parse({ projectId: project.id, sourceLineId: expenseLineId, amount: "75.00" }), key());
  expect(prepared).toMatchObject({ state: "draft", version: 1 });
  await expect(decideProjectCostAllocation(f.owner, prepared.id, projectCostAllocationDecisionSchema.parse({ action: "approve", expectedVersion: 1, reason: "Creator cannot approve the same allocation." }), key()))
    .rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  const posted = await decideProjectCostAllocation(f.manager, prepared.id, projectCostAllocationDecisionSchema.parse({ action: "approve", expectedVersion: 1, reason: "Controller approved the posted expense allocation." }), key());
  expect(posted).toMatchObject({ state: "approved", version: 2 });
  expect((await listProjectCostAllocations(f.owner, project.id))[0]).toMatchObject({ amount: "75.00", currency: "THB", journal_status: "posted", is_creator: true, can_review: false });
  expect((await listProjectCostAllocations(f.manager, project.id))[0]).toMatchObject({ is_creator: false, can_review: true });
  expect((await listProjectCostSources(f.reviewer, project.id))[0]).toMatchObject({ remaining: "25.00" });
  const secondProject = await saveProject(db, f.owner, null, projectSchema.parse({ title: "Second executive cost fixture", memberUserIds: [f.reviewer.userId] }), key());
  await expect(createProjectCostAllocation(f.reviewer, projectCostAllocationSchema.parse({ projectId: secondProject.id, sourceLineId: expenseLineId, amount: "26.00" }), key()))
    .rejects.toBeTruthy();

  const actor = { ...f.owner, modules: ["projects"] }, filters = { from: "2026-09-01", to: "2026-09-30", currency: "THB" };
  expect(await queryMetric(db, actor, "project-posted-cost", filters)).toMatchObject({ total: "7500", unit: "money", currency: "THB", status: "ready" });
  const search = await searchExecutiveEvidence(db, actor, { metric: "project-posted-cost", filters, query: "Executive actual-cost", limit: 10 });
  expect(search).toMatchObject({ provider: "projects-approved-actuals", metric: "project-posted-cost" });
  expect(search.items).toHaveLength(1);
  const detail = await readExecutiveEvidence(db, actor, "project-posted-cost", search.items[0].id, filters);
  expect(detail.record).toMatchObject({ journal_id: journalId, amount_minor: "7500", account_code: "expense", version: 2 });
  expect(detail.fields).not.toContain("description");

  await db.prepare("UPDATE finance_journals SET status='reversed',version=version+1,updated_at=? WHERE id=?").bind(Date.now(), journalId).run();
  expect(await queryMetric(db, actor, "project-posted-cost", filters)).toMatchObject({ total: "0", status: "empty" });
  await expect(readExecutiveEvidence(db, actor, "project-posted-cost", search.items[0].id, filters)).rejects.toMatchObject({ code: "EVIDENCE_NOT_FOUND" });
});
