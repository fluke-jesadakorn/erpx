import { sealSecret } from "../src/server/iam-mfa";
import { loadIamAuthority } from "../src/server/iam-store";
import { selectWorkRole } from "../src/server/work-role-store";
import { decideIamReview,validateIamGrant,grantInput } from "../src/server/iam-service";
import { iamActorFromWork } from "../src/server/iam-context";
import { drainSocialAgent, retryAgentReply } from "../src/server/social-agent";
import { ApiError } from "../src/server/http";
import { textSuggestions } from "../src/server/social-text";
import type { Mention } from "../src/lib/social-text";
import { plainRichContent } from "../src/lib/home-rich-content";
import { env } from "./runtime-env";
import { applyMigrations, type TestMigration } from "./runtime-env";
import { beforeAll, beforeEach, describe, it, expect, vi } from "vitest";
import {
  onboard,
  memberStatements,
  branchStatements,
} from "../src/server/beta-onboarding";
import {
  postView,
  postComments,
  homePosts,
  savePostDraft,
  publishPost,
  postAction,
  postRevisionHistory,
  socialUploadOwner,
  createCommentUploadDraft,
  createPostEditUploadDraft,
  type PostContent,
} from "../src/server/social-home";
import { sourceSnapshot, readablePost } from "../src/server/social-source";
import { createAiDraft, aiResultSchema, callAi } from "../src/server/social-ai";
import {
  saveTask,
  taskSchema,
  taskDetail,
  transitionTask,
} from "../src/server/staff-tasks";
import { betaDestinationAllowed } from "../src/server/beta-access";
import {
  addBetaBranch,
  inviteMember,
  acceptMember,
} from "../src/server/beta-team";
import {
  resourceSlots,
  registeredDatabases,
  registeredResources,
} from "../src/lib/holding-registry";
import type { WorkContext } from "../src/server/authorization";
vi.mock("../src/server/runtime", () => ({ runtimeEnv: async () => env, holdingResources: async () => ({write:env.H001_DB,read:env.H001_READ_DB,files:env.H001_FILES}) }));
const db = env.H001_DB,
  control = env.CONTROL_DB,
  key = () => crypto.randomUUID();
let company: string,
  branch: string,
  owner: WorkContext,
  worker: WorkContext,
  reviewer: WorkContext;
const registry = JSON.stringify([
  {
    id: "testbeta1",
    write: "TEST_ONE_WRITE",
    read: "TEST_ONE_READ",
    files: "TEST_ONE_FILES",
    beta: true,
  },
  {
    id: "testbeta2",
    write: "TEST_TWO_WRITE",
    read: "TEST_TWO_READ",
    files: "TEST_TWO_FILES",
    beta: true,
  },
]);
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS as TestMigration[]);
  await applyMigrations(
    env.H002_DB,
    env.STAFF_TEST_MIGRATIONS as TestMigration[],
  );
  await applyMigrations(
    control,
    env.CONTROL_TEST_MIGRATIONS as TestMigration[],
  );
});
async function freshIam(actor:WorkContext){
 const now=Date.now(),sessionId=key();
 await control.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?) ON CONFLICT DO NOTHING').bind(actor.userId,actor.name,actor.email,now,now).run();
 await control.prepare('INSERT INTO session(id,"expiresAt",token,"createdAt","updatedAt","userId") VALUES(?,?,?,?,?,?)').bind(sessionId,new Date(now+86400000).toISOString(),key(),new Date().toISOString(),new Date().toISOString(),actor.userId).run();
 await control.prepare('INSERT INTO iam_mfa(user_id,secret,enabled) VALUES(?,?,true) ON CONFLICT DO NOTHING').bind(actor.userId,await sealSecret('JBSWY3DPEHPK3PXP')).run();
 await control.prepare('INSERT INTO iam_mfa_sessions VALUES(?,?,?)').bind(sessionId,actor.userId,now).run();
 await selectWorkRole(db,{sessionId,userId:actor.userId,companyId:actor.companyId,assignmentId:actor.assignmentId,assignmentVersion:1,revision:0,expiresAt:now+86400000});
 actor.authority={sessionId,assignmentVersion:1,revision:1};actor.iam=await loadIamAuthority(db,actor.companyId,actor.userId,actor.role);return actor;
}
async function approveInvitation(){
 const id=key(),assignmentId=key(),second={...owner,userId:id,name:'Independent reviewer',email:id+'@example.test',assignmentId,authority:undefined};
 await db.prepare("INSERT INTO assignments(id,user_id,role,company_id,scope_kind,modules_json,valid_from,valid_until) VALUES(?,?,'holding_owner',?,'holding','[]',0,?)").bind(assignmentId,id,company,Date.now()+100*86400000).run();
 await freshIam(second);
 const pending=await db.prepare("SELECT id FROM iam_reviews WHERE company_id=? AND kind='beta.invitation' AND status='pending'").bind(company).first<{id:string}>();
 const actor=iamActorFromWork(second);await decideIamReview(db,actor,pending!.id,{expectedVersion:1,decision:'approve',reason:'Verified invitation role and expiry'},key(),async(c,kind,p)=>{expect(kind).toBe('beta.invitation');for(const grant of p.grants as unknown[])await validateIamGrant(c,actor,grantInput.parse(grant),String(p.parentId));});
}
beforeEach(async () => {
  company = `co-${key()}`;
  branch = `br-${key()}`;
  await db.batch([
    db
      .prepare(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Test company',?)",
      )
      .bind(company, company, company),
    ...branchStatements(db, company, branch, "Branch", branch),
    db
      .prepare(
        "INSERT INTO staff_levels(id,company_id,code,name,rank,created_at,updated_at) VALUES(?,?,'member','Member',1,1,1)",
      )
      .bind(`level-${company}`, company),
    db
      .prepare("INSERT INTO beta_policy VALUES(?,20,5,100,1073741824,1)")
      .bind(company),
  ]);
  const staff = async (name: string, role: string): Promise<WorkContext> => {
    const id = key();
    await db.batch(
      memberStatements(
        db,
        company,
        branch,
        { id, name, email: `${id}@example.test`, emailVerified: true },
        role === "holding_owner",
        null,
      ),
    );
    return {
      userId: id,
      name,
      email: `${id}@example.test`,
      assignmentId: `beta-assignment-${id}`,
      role,
      companyId: company,
      branchId: role === "holding_owner" ? null : branch,
      departmentId: null,
      holdingId: "h001",
      holdingSlug: "acme",
      holdingName: "Test",
      scope: {
        kind: "holding",
        holdingId: "h001",
        holdingSlug: "acme",
        holdingName: "Test",
        companyId: company,
      },
      assignmentScope: role === "holding_owner" ? "holding" : "branch",
      inherited: false,
      modules: [],
      assignments: [],
      workspaces: [],
    };
  };
  owner = await freshIam(await staff("Owner", "holding_owner"));
  worker = await staff("Worker", "employee");
  reviewer = await staff("Reviewer", "employee");
});
async function post() {
  const draft = await savePostDraft(
    db,
    owner,
    null,
    {
      body: "เครื่องปรับอากาศเสีย กรุณาช่วยตรวจสอบ",
      branchId: branch,
      expectedVersion: 1,
    },
    key(),
  );
  return publishPost(db, owner, draft.id, draft.version, key());
}
async function readySocialImage(draftId: string, name: string) {
  const uploadId = key(), attachmentId = key(), now = Date.now();
  const objectKey = `test/social/${uploadId}`;
  await db.batch([
    db.prepare(`INSERT INTO upload_intents
      (id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,revision,status,created_by,expires_at,created_at,updated_at)
      VALUES(?,'social_draft',?,'post-image',?,'image/png',1,1,?,?,?,1,'ready',?,?,?,?)`)
      .bind(uploadId,draftId,name,"test-checksum",`${objectKey}/staging`,objectKey,owner.userId,now+900_000,now,now),
    db.prepare(`INSERT INTO attachments
      (id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at)
      VALUES(?,?,'social_draft',?,'post-image',?,1,?,'image/png',1,?,?,?)`)
      .bind(attachmentId,uploadId,draftId,objectKey,name,"test-checksum",owner.userId,now),
  ]);
  return { uploadId, attachmentId };
}
async function source(id: string) {
  const snap = await sourceSnapshot(db, owner, id, []);
  return {
    postId: id,
    sourceVersion: snap.post.version,
    sourceHash: snap.hash,
    commentIds: [],
    aiDraftId: null,
  };
}
function task() {
  return taskSchema.parse({
    title: "Repair air conditioner",
    description: "Inspect and attach evidence",
    assigneeUserId: worker.userId,
    dueAt: "2026-10-01T12:00:00+07:00",
    completionApprovers: [reviewer.userId],
  });
}
describe("social post image content", () => {
  it("keeps image order across draft save and published post views", async () => {
    const firstSlot = key(), secondSlot = key();
    const pending: PostContent = [
      { type: "text", text: "Before" },
      { type: "image", slotId: firstSlot, attachmentId: null, name: "first.png" },
      { type: "image", slotId: secondSlot, attachmentId: null, name: "second.png" },
      { type: "text", text: "After" },
    ];
    const draft = await savePostDraft(db, owner, null, {
      body: "Before\nAfter", content: pending, branchId: branch, expectedVersion: 1,
    }, key());
    const first = await readySocialImage(draft.id, "first.png");
    const second = await readySocialImage(draft.id, "second.png");
    const content: PostContent = [
      { type: "text", text: "Before" },
      { type: "image", slotId: firstSlot, attachmentId: first.attachmentId, name: "first.png" },
      { type: "image", slotId: secondSlot, attachmentId: second.attachmentId, name: "second.png" },
      { type: "text", text: "After" },
    ];
    const saved = await savePostDraft(db, owner, draft.id, {
      body: "Before\nAfter", content, branchId: branch, expectedVersion: draft.version,
    }, key());
    const stored = await db.prepare("SELECT content_json,body FROM social_post_drafts WHERE id=?")
      .bind(draft.id).first<{content_json:string;body:string}>();
    expect(stored?.body).toBe("Before\nAfter");
    expect(JSON.parse(stored!.content_json)).toEqual(content);
    const published = await publishPost(db, owner, draft.id, saved.version, key());
    const view = await postView(db, owner, published.id);
    expect(view.body).toBe("Before\nAfter");
    expect(view.content).toEqual(content);
    expect(view.files.map((file) => file.id).sort()).toEqual([first.attachmentId, second.attachmentId].sort());
  });

  it("publishes an image-only post only after its own upload is ready", async () => {
    const slotId = key();
    const pending: PostContent = [{ type: "image", slotId, attachmentId: null, name: "only.png" }];
    const draft = await savePostDraft(db, owner, null, {
      body: "", content: pending, branchId: branch, expectedVersion: 1,
    }, key());
    await expect(publishPost(db, owner, draft.id, draft.version, key())).rejects.toMatchObject({
      code: "POST_IMAGE_PENDING", status: 422,
    });
    const upload = await readySocialImage(draft.id, "only.png");
    const content: PostContent = [{ type: "image", slotId, attachmentId: upload.attachmentId, name: "only.png" }];
    const saved = await savePostDraft(db, owner, draft.id, {
      body: "", content, branchId: branch, expectedVersion: draft.version,
    }, key());
    await db.prepare("UPDATE upload_intents SET status='pending' WHERE id=?").bind(upload.uploadId).run();
    await expect(publishPost(db, owner, draft.id, saved.version, key())).rejects.toMatchObject({
      code: "POST_IMAGE_UNAVAILABLE", status: 422,
    });
    await db.prepare("UPDATE upload_intents SET status='ready' WHERE id=?").bind(upload.uploadId).run();
    const published = await publishPost(db, owner, draft.id, saved.version, key());
    const view = await postView(db, owner, published.id);
    expect(view.body).toBe("");
    expect(view.content).toEqual(content);
  });

  it("rejects another draft's image and preserves legacy text-only posts", async () => {
    const sourceDraft = await savePostDraft(db, owner, null, {
      body: "Source", branchId: branch, expectedVersion: 1,
    }, key());
    const foreignImage = await readySocialImage(sourceDraft.id, "foreign.png");
    const draft = await savePostDraft(db, owner, null, {
      body: "", content: [{ type: "image", slotId: key(), attachmentId: foreignImage.attachmentId, name: "foreign.png" }],
      branchId: branch, expectedVersion: 1,
    }, key());
    await expect(publishPost(db, owner, draft.id, draft.version, key())).rejects.toMatchObject({
      code: "POST_IMAGE_UNAVAILABLE", status: 422,
    });
    expect(await db.prepare("SELECT count(*) FROM social_posts WHERE id=?")
      .bind(`post-${draft.id}`).first<number>("count")).toBe(0);
    const legacy = await publishPost(db, owner, sourceDraft.id, sourceDraft.version, key());
    const view = await postView(db, owner, legacy.id);
    expect(view.body).toBe("Source");
    expect(view.content).toBeNull();
  });

  it("revokes an image removed from a draft before publication", async () => {
    const slotId = key();
    const draft = await savePostDraft(db, owner, null, {
      body: "Keep this text", branchId: branch, expectedVersion: 1,
    }, key());
    const image = await readySocialImage(draft.id, "removed.png");
    const withImage = await savePostDraft(db, owner, draft.id, {
      body: "Keep this text",
      content: [
        { type: "text", text: "Keep this text" },
        { type: "image", slotId, attachmentId: image.attachmentId, name: "removed.png" },
      ],
      branchId: branch, expectedVersion: draft.version,
    }, key());
    const withoutImage = await savePostDraft(db, owner, draft.id, {
      body: "Keep this text", content: [{ type: "text", text: "Keep this text" }],
      branchId: branch, expectedVersion: withImage.version,
    }, key());
    expect(await db.prepare("SELECT revoked_at IS NOT NULL AS revoked FROM attachments WHERE id=?")
      .bind(image.attachmentId).first<boolean>("revoked")).toBe(true);
    const published = await publishPost(db, owner, draft.id, withoutImage.version, key());
    const view = await postView(db, owner, published.id);
    expect(view.content).toEqual([{ type: "text", text: "Keep this text" }]);
    expect(view.files).toHaveLength(0);
  });
  it("publishes a comment image from a private upload draft and records the reply thread",async()=>{
    const published=await post();
    const draft=await createCommentUploadDraft(db,owner,published.id,key());
    const image=await readySocialImage(draft.id,"machine.png");
    const rich=plainRichContent("What is shown in this photo?");
    rich.blocks.push({type:"image",slotId:key(),attachmentId:image.attachmentId,name:"machine.png",alt:"Machine label"});
    await postAction(db,owner,published.id,{action:"comment",body:"What is shown in this photo?",richContent:rich,commentDraftId:draft.id},key());
    const comments=await postComments(db,worker,published.id);
    const comment=comments.items.find(item=>item.body==="What is shown in this photo?");
    expect(comment?.richContent?.blocks[1]).toMatchObject({type:"image",attachmentId:image.attachmentId});
    expect((await socialUploadOwner(db,worker,draft.id)).published_comment_id).toBe(comment?.id);
    await expect(socialUploadOwner(db,owner,draft.id,true)).rejects.toMatchObject({code:"DRAFT_LOCKED"});
  });
});
describe("social post audience and purpose", () => {
  it("requires an audience before publication and preserves the selected purpose", async () => {
    const saved = await savePostDraft(db, owner, null, {
      body: "Tomorrow's delivery plan", purpose: "plan", expectedVersion: 1,
    }, key());
    await expect(publishPost(db, owner, saved.id, saved.version, key())).rejects.toMatchObject({
      code: "AUDIENCE_REQUIRED", status: 422,
    });
    const selected = await savePostDraft(db, owner, saved.id, {
      body: "Tomorrow's delivery plan", purpose: "plan",
      audience: { kind: "branch", id: branch }, expectedVersion: saved.version,
    }, key());
    const published = await publishPost(db, owner, saved.id, selected.version, key());
    const view = await postView(db, worker, published.id);
    expect(view.purpose).toBe("plan");
    expect(view.audience_kind).toBe("branch");
    expect(view.audienceLabel).toBe("Branch · Branch");
  });

  it("prevents branch roles from widening a post to the company or another branch", async () => {
    await expect(savePostDraft(db, worker, null, {
      body: "Private branch work", audience: { kind: "company" }, expectedVersion: 1,
    }, key())).rejects.toMatchObject({ code: "AUDIENCE_FORBIDDEN", status: 403 });
    await expect(savePostDraft(db, worker, null, {
      body: "Private branch work", audience: { kind: "branch", id: `other-${key()}` }, expectedVersion: 1,
    }, key())).rejects.toMatchObject({ code: "AUDIENCE_FORBIDDEN", status: 403 });
  });

  it("keeps a department audience out of the branch feed", async () => {
    const department = `dept-${key()}`, id = `post-${key()}`, now = Date.now();
    await db.batch([
      db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'Sales',?,?,?)")
        .bind(department,department,branch,company,branch),
      db.prepare("INSERT INTO social_posts(id,kind,author_id,author_name,company_id,branch_id,department_id,audience_kind,purpose,body,created_at,updated_at) VALUES(?,'team',?,?,?,?,?,'department','update','Department only',?,?)")
        .bind(id,owner.userId,owner.name,company,branch,department,now,now),
    ]);
    await expect(readablePost(db, worker, id)).rejects.toMatchObject({ code: "POST_NOT_FOUND", status: 404 });
    await db.prepare("UPDATE assignments SET scope_kind='department',scope_node_id=? WHERE id=?")
      .bind(department,worker.assignmentId).run();
    await expect(readablePost(db, worker, id)).resolves.toMatchObject({ id });
  });

  it("limits selected-person posts to named recipients and keeps mentions from adding viewers", async () => {
    for (const actor of [worker, reviewer]) await control.batch([
      control.prepare('INSERT INTO "user"(id,name,email,"createdAt","updatedAt") VALUES(?,?,?,1,1) ON CONFLICT(id) DO NOTHING')
        .bind(actor.userId, actor.name, actor.email),
      control.prepare("INSERT INTO principals(id,user_id,display_name,created_at) VALUES(?,?,?,1) ON CONFLICT(id) DO NOTHING")
        .bind(actor.userId, actor.userId, actor.name),
    ]);
    const body = "@Reviewer please read the update";
    const draft = await savePostDraft(db, owner, null, {
      body, mentions: [{ kind: "person", targetId: reviewer.userId, label: "Reviewer", start: 0, end: 9 }],
      audience: { kind: "participants", userIds: [worker.userId] }, expectedVersion: 1,
    }, key());
    const published = await publishPost(db, owner, draft.id, draft.version, key());
    expect((await postView(db, worker, published.id)).audienceLabel).toBe("Selected people (1)");
    await expect(readablePost(db, reviewer, published.id)).rejects.toMatchObject({ code: "POST_NOT_FOUND" });
    expect(await db.prepare("SELECT count(*) FROM staff_notifications WHERE record_id=? AND user_id=?")
      .bind(published.id, reviewer.userId).first<number>("count")).toBe(0);
    await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(), worker.assignmentId).run();
    await expect(readablePost(db, worker, published.id)).rejects.toMatchObject({ code: "POST_NOT_FOUND" });
  });

  it("checks current team and project membership within the original branch scope", async () => {
    const team = `team-${key()}`, project = `project-${key()}`, now = Date.now();
    await db.batch([
      db.prepare("INSERT INTO messaging_teams(id,company_id,name,owner_id,created_at) VALUES(?,?,?,?,?)")
        .bind(team, company, "Service team", worker.userId, now),
      db.prepare("INSERT INTO messaging_team_members(team_id,user_id) VALUES(?,?)").bind(team, worker.userId),
      db.prepare("INSERT INTO messaging_team_members(team_id,user_id) VALUES(?,?)").bind(team, reviewer.userId),
      db.prepare("INSERT INTO staff_projects(id,company_id,branch_id,title,owner_user_id,created_at,updated_at) VALUES(?,?,?,?,?,?,?)")
        .bind(project, company, branch, "Opening project", worker.userId, now, now),
      db.prepare("INSERT INTO staff_project_members(project_id,user_id,added_by,created_at) VALUES(?,?,?,?)")
        .bind(project, reviewer.userId, worker.userId, now),
    ]);
    const otherBranch = `other-${key()}`;
    await db.batch(branchStatements(db, company, otherBranch, "Other", otherBranch));
    await db.prepare("UPDATE assignments SET scope_node_id=? WHERE id=?").bind(otherBranch, reviewer.assignmentId).run();
    for (const audience of [{ kind: "team" as const, id: team }, { kind: "project" as const, id: project }]) {
      const draft = await savePostDraft(db, worker, null, { body: "Branch handover", audience, expectedVersion: 1 }, key());
      const published = await publishPost(db, worker, draft.id, draft.version, key());
      expect((await postView(db, worker, published.id)).audience_kind).toBe(audience.kind);
      await expect(readablePost(db, reviewer, published.id)).rejects.toMatchObject({ code: "POST_NOT_FOUND" });
      await db.prepare("UPDATE assignments SET scope_node_id=? WHERE id=?").bind(branch, reviewer.assignmentId).run();
      await expect(readablePost(db, reviewer, published.id)).resolves.toMatchObject({ id: published.id });
      await db.prepare("UPDATE assignments SET scope_node_id=? WHERE id=?").bind(otherBranch, reviewer.assignmentId).run();
    }
    await db.prepare("UPDATE assignments SET scope_node_id=? WHERE id=?").bind(branch, reviewer.assignmentId).run();
    const draft = await savePostDraft(db, worker, null, { body: "Team note", audience: { kind: "team", id: team }, expectedVersion: 1 }, key());
    const published = await publishPost(db, worker, draft.id, draft.version, key());
    await db.prepare("DELETE FROM messaging_team_members WHERE team_id=? AND user_id=?").bind(team, reviewer.userId).run();
    await expect(readablePost(db, reviewer, published.id)).rejects.toMatchObject({ code: "POST_NOT_FOUND" });
  });

  it("shows each linked task or project only to readers authorized by its own source", async () => {
    const task = `task-${key()}`, project = `project-${key()}`, now = Date.now();
    await db.batch([
      db.prepare("INSERT INTO staff_tasks(id,company_id,branch_id,title,creator_user_id,assignee_user_id,due_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)")
        .bind(task, company, branch, "Restricted repair", owner.userId, worker.userId, now + 86400000, now, now),
      db.prepare("INSERT INTO staff_projects(id,company_id,title,owner_user_id,created_at,updated_at) VALUES(?,?,?,?,?,?)")
        .bind(project, company, "Private rollout", owner.userId, now, now),
    ]);
    const draft = await savePostDraft(db, owner, null, {
      body: "Linked work", audience: { kind: "company" },
      sources: [{ kind: "staff_task", id: task }, { kind: "staff_project", id: project }], expectedVersion: 1,
    }, key());
    const published = await publishPost(db, owner, draft.id, draft.version, key());
    expect((await postView(db, owner, published.id)).linkedSources).toMatchObject([
      { available: true, title: "Restricted repair" }, { available: true, title: "Private rollout" },
    ]);
    const limited = (await postView(db, reviewer, published.id)).linkedSources;
    expect(limited).toEqual([
      { kind: "staff_task", available: false }, { kind: "staff_project", available: false },
    ]);
    await expect(savePostDraft(db, reviewer, null, {
      body: "Cannot link someone else's task", audience: { kind: "branch", id: branch },
      sources: [{ kind: "staff_task", id: task }], expectedVersion: 1,
    }, key())).rejects.toMatchObject({ code: "TASK_NOT_FOUND", status: 404 });
  });
});
describe("social post corrections", () => {
  it("lets only the author edit text while preserving audience and revision history", async () => {
    const created = await post();
    await expect(postAction(db, worker, created.id, {
      action: "edit", expectedVersion: 1, body: "Incorrect correction",
    }, key())).rejects.toMatchObject({ code: "POST_AUTHOR_REQUIRED", status: 403 });
    const changed = await postAction(db, owner, created.id, {
      action: "edit", expectedVersion: 1, body: "Corrected #handover",
    }, key());
    expect(changed.version).toBe(2);
    const view = await postView(db, worker, created.id);
    expect(view.body).toBe("Corrected #handover");
    expect(view.branch_id).toBe(branch);
    expect(view.audienceLabel).toBe("Branch · Branch");
    expect(view.edited_at).toBeGreaterThan(0);
    expect(await postRevisionHistory(db, owner, created.id)).toMatchObject([
      { prior_version: 1, action: "edit", body: "เครื่องปรับอากาศเสีย กรุณาช่วยตรวจสอบ" },
    ]);
    await expect(postRevisionHistory(db, worker, created.id)).rejects.toMatchObject({ status: 404 });
    await expect(postAction(db, owner, created.id, {
      action: "edit", expectedVersion: 1, body: "Stale correction",
    }, key())).rejects.toMatchObject({ code: "POST_CHANGED", status: 409 });
  });

  it("retracts a post without deleting its audit trail", async () => {
    const created = await post();
    await expect(postAction(db, worker, created.id, {
      action: "retract", expectedVersion: 1,
    }, key())).rejects.toMatchObject({ code: "POST_AUTHOR_REQUIRED", status: 403 });
    const commandKey = key();
    const deleted = await postAction(db, owner, created.id, { action: "retract", expectedVersion: 1 }, commandKey);
    expect(await postAction(db, owner, created.id, { action: "retract", expectedVersion: 1 }, commandKey)).toEqual(deleted);
    await expect(postView(db, owner, created.id)).rejects.toMatchObject({ code: "POST_NOT_FOUND", status: 404 });
    expect((await homePosts(db, owner)).some(row => row.id === created.id)).toBe(false);
    expect(await postRevisionHistory(db, owner, created.id)).toMatchObject([
      { prior_version: 1, action: "retract", body: "เครื่องปรับอากาศเสีย กรุณาช่วยตรวจสอบ" },
    ]);
    const audit = await db.prepare("SELECT count(*) FROM audit_events WHERE record_id=? AND action='social.post.retract'")
      .bind(created.id).first<number>("count");
    expect(audit).toBe(1);
  });

  it("edits image order and ownership through a versioned upload draft", async () => {
    const first = { type: "image" as const, slotId: "original-one", attachmentId: null, name: "one.png", alt: "First" };
    const second = { type: "image" as const, slotId: "original-two", attachmentId: null, name: "two.png", alt: "Second" };
    const initial: PostContent = [
      { type: "text", text: "Before" }, first, { type: "text", text: "Between" }, second, { type: "text", text: "After" },
    ];
    const rich = (content: PostContent) => ({ version: 1 as const, blocks: content.map(block => block.type === "image"
      ? { type: "image" as const, slotId: block.slotId, attachmentId: block.attachmentId, name: block.name, alt: block.alt, width: block.width }
      : { type: "text" as const, doc: plainRichContent(block.text).blocks[0].doc }) });
    let draft = await savePostDraft(db, owner, null, {
      body: "Before\nBetween\nAfter", content: initial, richContent: rich(initial), audience: { kind: "branch", id: branch }, expectedVersion: 1,
    }, key());
    const uploadOne = await readySocialImage(draft.id, "one.png");
    const uploadTwo = await readySocialImage(draft.id, "two.png");
    const publishedContent: PostContent = initial.map(block => block.type === "image"
      ? { ...block, attachmentId: block.slotId === first.slotId ? uploadOne.attachmentId : uploadTwo.attachmentId }
      : block);
    draft = await savePostDraft(db, owner, draft.id, {
      body: "Before\nBetween\nAfter", content: publishedContent, richContent: rich(publishedContent),
      audience: { kind: "branch", id: branch }, expectedVersion: draft.version,
    }, key());
    const created = await publishPost(db, owner, draft.id, draft.version, key());
    const uploadDraft = await createPostEditUploadDraft(db, owner, created.id, 1, key());
    await expect(socialUploadOwner(db, worker, uploadDraft.id, true)).rejects.toMatchObject({ status: 403 });

    const newImage = { type: "image" as const, slotId: "added", attachmentId: "missing-file", name: "three.png", alt: "Third" };
    const editContent: PostContent = [publishedContent[0], publishedContent[3], publishedContent[2], newImage, publishedContent[4]];
    await expect(postAction(db, owner, created.id, {
      action: "edit", expectedVersion: 1, body: "Before\nBetween\nAfter", mentions: [], content: editContent,
      richContent: rich(editContent), editDraftId: uploadDraft.id,
    }, key())).rejects.toMatchObject({ code: "POST_IMAGE_UNAVAILABLE", status: 422 });

    const addedUpload = await readySocialImage(uploadDraft.id, "three.png");
    editContent[3] = { ...newImage, attachmentId: addedUpload.attachmentId };
    const edited = await postAction(db, owner, created.id, {
      action: "edit", expectedVersion: 1, body: "Before\nBetween\nAfter", mentions: [], content: editContent,
      richContent: rich(editContent), editDraftId: uploadDraft.id,
    }, key());
    expect(edited.version).toBe(2);
    const view = await postView(db, owner, created.id);
    expect(view.content?.map(block => block.type === "image" ? block.attachmentId : block.text)).toEqual([
      "Before", uploadTwo.attachmentId, "Between", addedUpload.attachmentId, "After",
    ]);
    expect(view.files.map(file => file.id)).toEqual([uploadTwo.attachmentId, addedUpload.attachmentId]);
    await expect(postAction(db, owner, created.id, {
      action: "edit", expectedVersion: 1, body: "stale", mentions: [], content: [{ type: "text", text: "stale" }],
    }, key())).rejects.toMatchObject({ code: "POST_CHANGED", status: 409 });
    await expect(postAction(db, worker, created.id, {
      action: "edit", expectedVersion: 2, body: "forged", mentions: [], content: [{ type: "text", text: "forged" }],
    }, key())).rejects.toMatchObject({ code: "POST_AUTHOR_REQUIRED", status: 403 });

    const photoOnly: PostContent = [publishedContent[3]];
    const imageOnly = await postAction(db, owner, created.id, {
      action: "edit", expectedVersion: 2, body: "", mentions: [], content: photoOnly, richContent: rich(photoOnly),
    }, key());
    expect(imageOnly.version).toBe(3);
    expect(await postView(db, owner, created.id)).toMatchObject({ body: "", content: photoOnly });
  });
});
describe("social conversation replies", () => {
  it("keeps replies attached to the selected comment while rendering one reply depth", async () => {
    const created = await post();
    const firstKey = key(), replyKey = key();
    await postAction(db, owner, created.id, { action: "comment", body: "Can someone check the switch?" }, firstKey);
    const parentId = `comment-${firstKey}`;
    await postAction(db, worker, created.id, {
      action: "comment", body: "I checked it this morning.", parentCommentId: parentId,
    }, replyKey);
    const view = await postView(db, owner, created.id);
    const reply = view.comments.find(comment => comment.id === `comment-${replyKey}`);
    expect(reply).toMatchObject({ parent_comment_id: parentId, parent_author_name: owner.name });
    const page = await postComments(db, owner, created.id);
    expect(page.items.find(comment => comment.id === `comment-${replyKey}`)?.parent_comment_id).toBe(parentId);
    const notification = await db.prepare("SELECT count(*) FROM staff_notifications WHERE record_id=? AND user_id=? AND message='Someone replied to your comment'")
      .bind(created.id, owner.userId).first<number>("count");
    expect(notification).toBe(1);
    const nestedKey = key();
    await postAction(db, reviewer, created.id, {
      action: "comment", body: "I will check with the team.", parentCommentId: `comment-${replyKey}`,
    }, nestedKey);
    const thread = await postView(db, owner, created.id);
    expect(thread.comments.find(comment => comment.id === `comment-${nestedKey}`))
      .toMatchObject({ parent_comment_id: `comment-${replyKey}`, parent_author_name: worker.name });
    expect(thread.comments.find(comment => comment.id === `comment-${replyKey}`)?.parent_comment_id).toBe(parentId);
    const older = await postComments(db, owner, created.id);
    expect(older.items.find(comment => comment.id === parentId)?.id).toBe(parentId);
  });
  it("lets an author delete a comment without deleting its replies", async () => {
    const created = await post();
    const parentKey = key();
    await postAction(db, owner, created.id, { action: "comment", body: "Original" }, parentKey);
    const commentId = `comment-${parentKey}`;
    await postAction(db, worker, created.id, { action: "comment", body: "Reply", parentCommentId: commentId }, key());
    await expect(postAction(db, worker, created.id, { action: "delete_comment", commentId }, key()))
      .rejects.toMatchObject({ status: 403 });
    const deleteKey = key();
    await postAction(db, owner, created.id, { action: "delete_comment", commentId }, deleteKey);
    await postAction(db, owner, created.id, { action: "delete_comment", commentId }, deleteKey);
    const view = await postView(db, owner, created.id);
    expect(view.comments.some(comment => comment.id === commentId)).toBe(false);
    expect(view.comments.some(comment => comment.body === "Reply")).toBe(true);
    expect(view.commentCount).toBe(1);
  });
  it("lets the author edit a comment and rejects another author", async () => {
    const created = await post();
    const createKey = key();
    await postAction(db, owner, created.id, { action: "comment", body: "Before" }, createKey);
    const commentId = `comment-${createKey}`;
    const body = "After";
    const richContent = plainRichContent(body, []);
    await expect(postAction(db, worker, created.id, { action: "edit_comment", commentId, body, richContent }, key()))
      .rejects.toMatchObject({ status: 403 });
    await postAction(db, owner, created.id, { action: "edit_comment", commentId, body, richContent }, key());
    expect((await postView(db, owner, created.id)).comments.find(comment => comment.id === commentId)?.body).toBe(body);
  });
});
describe("social beta isolation and work", () => {
  it("keeps drafts private, then uses the normal independent review workflow", async () => {
    const draft = await savePostDraft(
      db,
      owner,
      null,
      { body: "Need help", branchId: branch, expectedVersion: 1 },
      key(),
    );
    await expect(socialUploadOwner(db, worker, draft.id)).rejects.toMatchObject(
      { status: 403 },
    );
    const p = await publishPost(db, owner, draft.id, 1, key());
    await expect(
      socialUploadOwner(db, worker, draft.id),
    ).resolves.toMatchObject({ published_post_id: p.id });
    const created = await saveTask(
      db,
      owner,
      null,
      task(),
      key(),
      false,
      await source(p.id),
    );
    const id = String(created.id);
    expect((await taskDetail(db, owner, id)).task.state).toBe("draft");
    await transitionTask(
      db,
      owner,
      id,
      { action: "submit", expectedVersion: 1, note: "" },
      key(),
    );
    await transitionTask(
      db,
      worker,
      id,
      { action: "start", expectedVersion: 2, note: "" },
      key(),
    );
    await transitionTask(
      db,
      worker,
      id,
      {
        action: "complete",
        expectedVersion: 3,
        note: "Replaced the faulty switch. Tested.",
      },
      key(),
    );
    await expect(
      transitionTask(
        db,
        worker,
        id,
        { action: "approve", expectedVersion: 4, note: "My own work" },
        key(),
      ),
    ).rejects.toMatchObject({ status: 403 });
    await transitionTask(
      db,
      reviewer,
      id,
      { action: "approve", expectedVersion: 4, note: "Inspected and accepted" },
      key(),
    );
    expect((await taskDetail(db, owner, id)).task.state).toBe("completed");
    expect(
      await db
        .prepare("SELECT count(*) n FROM social_task_origins WHERE post_id=?")
        .bind(p.id)
        .first("n"),
    ).toBe(1);
  });
  it("commits exactly one task, source, receipt and assignment notification during a race", async () => {
    const p = await post(),
      s = await source(p.id);
    const outcomes = await Promise.allSettled([
      saveTask(db, owner, null, task(), key(), false, s),
      saveTask(db, owner, null, task(), key(), false, s),
    ]);
    expect(outcomes.filter((x) => x.status === "fulfilled")).toHaveLength(1);
    const id = `task-post-${p.id}`;
    expect(
      await db
        .prepare("SELECT count(*) n FROM staff_tasks WHERE id=?")
        .bind(id)
        .first("n"),
    ).toBe(1);
    expect(
      await db
        .prepare(
          "SELECT count(*) n FROM audit_events WHERE record_id=? AND action='task.create'",
        )
        .bind(id)
        .first("n"),
    ).toBe(1);
    expect(
      await db
        .prepare(
          "SELECT count(*) n FROM outbox_events WHERE aggregate_id=? AND event_type='task.create'",
        )
        .bind(id)
        .first("n"),
    ).toBe(1);
  });
  it("rechecks source version and permissions before any task writes", async () => {
    const p = await post(),
      s = await source(p.id);
    await postAction(
      db,
      worker,
      p.id,
      { action: "comment", body: "New information" },
      key(),
    );
    await expect(
      saveTask(db, owner, null, task(), key(), false, s),
    ).rejects.toMatchObject({ code: "SOURCE_CHANGED" });
    expect(
      await db
        .prepare("SELECT task_id FROM social_task_origins WHERE post_id=?")
        .bind(p.id)
        .first(),
    ).toBeNull();
    await db
      .prepare("UPDATE assignments SET revoked_at=1 WHERE user_id=?")
      .bind(worker.userId)
      .run();
    await expect(
      saveTask(db, owner, null, task(), key(), false, await source(p.id)),
    ).rejects.toMatchObject({ status: 404 });
  });
  it("denies another company, another branch, revoked accounts and hidden source files", async () => {
    const p = await post();
    await expect(
      readablePost(db, { ...worker, companyId: "another-company" }, p.id),
    ).rejects.toMatchObject({ status: 404 });
    await expect(readablePost(env.H002_DB, worker, p.id)).rejects.toMatchObject(
      { status: 404 },
    );
    await db
      .prepare(
        "UPDATE assignments SET scope_node_id='other-branch' WHERE user_id=?",
      )
      .bind(worker.userId)
      .run();
    await expect(readablePost(db, worker, p.id)).rejects.toMatchObject({
      status: 404,
    });
    await postAction(db, owner, p.id, { action: "hide" }, key());
    await expect(readablePost(db, owner, p.id)).rejects.toMatchObject({
      status: 404,
    });
  });
  it("rejects idempotency keys reused by another actor and leaves no extra posts", async () => {
    const k = key(),
      input = { body: "Private", branchId: branch, expectedVersion: 1 };
    await savePostDraft(db, owner, null, input, k);
    await expect(
      savePostDraft(db, worker, null, input, k),
    ).rejects.toMatchObject({ code: "IDEMPOTENCY_CONFLICT" });
  });
  it("keeps beta modules unavailable at server destinations", () => {
    for (const path of [
      "finance",
      "people/payroll",
      "modules",
      "records/search",
      "posts",
      "posts/a/comments",
    ])
      expect(betaDestinationAllowed(path, true)).toBe(false);
    expect(betaDestinationAllowed("posts/a/task", true)).toBe(true);
    expect(betaDestinationAllowed("staff-tasks/a/transition", true)).toBe(true);
    expect(betaDestinationAllowed("people", false)).toBe(true);
    expect(betaDestinationAllowed("staff/levels", true)).toBe(true);
    expect(betaDestinationAllowed("ai-providers", true)).toBe(true);
    expect(betaDestinationAllowed("expenses", true)).toBe(true);
  });
});
describe("AI reservations and private output", () => {
  const output = (postId: string) => ({
    result: {
      summary: "Air conditioning needs inspection.",
      title: null,
      description: null,
      assigneeUserId: null,
      reviewerUserId: null,
      dueAt: null,
      missing: ["Assignee", "Due date"],
      citations: [postId],
    },
    inputTokens: 10,
    outputTokens: 5,
  });
  it("reserves before calling, validates references and persists only a private draft", async () => {
    const p = await post();
    const provider = vi.fn(async () => output(p.id));
    const draft = await createAiDraft(
      db,
      owner,
      p.id,
      { kind: "task", commentIds: [] },
      { AI_MODEL: "test" },
      provider,
    );
    expect(draft.result.assigneeUserId).toBeNull();
    expect(provider).toHaveBeenCalledOnce();
    expect(
      await db
        .prepare("SELECT actor_id FROM social_ai_drafts WHERE id=?")
        .bind(draft.id)
        .first("actor_id"),
    ).toBe(owner.userId);
    expect(
      await db
        .prepare("SELECT task_id FROM social_task_origins WHERE post_id=?")
        .bind(p.id)
        .first(),
    ).toBeNull();
  });
  it("atomically limits concurrent requests and monthly quota", async () => {
    const p = await post();
    await db
      .prepare("UPDATE beta_policy SET ai_monthly_limit=1 WHERE company_id=?")
      .bind(company)
      .run();
    const provider = vi.fn(async () => output(p.id));
    const results = await Promise.allSettled([
      createAiDraft(
        db,
        owner,
        p.id,
        { kind: "task", commentIds: [] },
        { AI_MODEL: "test" },
        provider,
      ),
      createAiDraft(
        db,
        owner,
        p.id,
        { kind: "task", commentIds: [] },
        { AI_MODEL: "test" },
        provider,
      ),
    ]);
    expect(results.filter((r) => r.status === "fulfilled")).toHaveLength(1);
    expect(provider).toHaveBeenCalledOnce();
  });
  it.each(["timeout", "schema", "citation", "revocation", "changed"] as const)(
    "fails safely for %s without creating work",
    async (failure) => {
      const p = await post();
      const provider = async () => {
        if (failure === "timeout") throw new Error("Timeout");
        if (failure === "revocation")
          await db
            .prepare("UPDATE assignments SET revoked_at=1 WHERE id=?")
            .bind(owner.assignmentId)
            .run();
        if (failure === "changed")
          await postAction(
            db,
            worker,
            p.id,
            { action: "comment", body: "Changed" },
            key(),
          );
        const result = output(p.id);
        if (failure === "schema") result.result.summary = "x".repeat(3001);
        if (failure === "citation") result.result.citations = ["foreign-post"];
        return result;
      };
      await expect(
        createAiDraft(
          db,
          owner,
          p.id,
          { kind: "task", commentIds: [] },
          { AI_MODEL: "test" },
          provider,
        ),
      ).rejects.toMatchObject({ code: "AI_UNAVAILABLE" });
      expect(
        await db
          .prepare("SELECT task_id FROM social_task_origins WHERE post_id=?")
          .bind(p.id)
          .first(),
      ).toBeNull();
      expect(
        await db
          .prepare("SELECT state FROM social_ai_usage WHERE company_id=?")
          .bind(company)
          .first("state"),
      ).toBe("failed");
    },
  );
  it("handles provider refusal, invalid JSON and excessive output", async () => {
    const original = globalThis.fetch;
    try {
      for (const payload of [
        {
          choices: [{ finish_reason: "stop", message: { refusal: "Refused" } }],
        },
        { choices: [{ finish_reason: "length", message: { content: "{}" } }] },
        {
          choices: [
            { finish_reason: "stop", message: { content: "not json" } },
          ],
        },
      ]) {
        globalThis.fetch = vi.fn(
          async () => new Response(JSON.stringify(payload), { status: 200 }),
        );
        await expect(
          callAi(
            {
              AI_BASE_URL: "https://provider.example/v1",
              AI_API_KEY: "test",
              AI_MODEL: "test",
            },
            {},
          ),
        ).rejects.toThrow();
      }
    } finally {
      globalThis.fetch = original;
    }
  });
  it("requires explicit missing task fields in the schema", () => {
    expect(aiResultSchema.safeParse({ summary: "Incomplete" }).success).toBe(
      false,
    );
  });
});
describe("tenant admission and verified invitations", () => {
  it("resumes a claim and admits only available isolated slots", async () => {
    await applyMigrations(
      env.H001_READ_DB,
      env.STAFF_TEST_MIGRATIONS as TestMigration[],
    );
    await applyMigrations(
      env.H002_READ_DB,
      env.STAFF_TEST_MIGRATIONS as TestMigration[],
    );
    const betaEnv = {
      ...env,
      TEST_ONE_WRITE: env.H001_READ_DB,
      TEST_ONE_READ: env.H001_DB,
      TEST_ONE_FILES: env.H001_FILES,
      TEST_TWO_WRITE: env.H002_READ_DB,
      TEST_TWO_READ: env.H002_DB,
      TEST_TWO_FILES: env.H002_FILES,
      ERPX_RESOURCE_SLOTS: registry,
      ERPX_PUBLIC_BETA: "true",
    };
    const users = await Promise.all(
      [1, 2, 3].map(async (n) => {
        const u = {
          id: `onboard-${key()}`,
          name: `Owner ${n}`,
          email: `${key()}@example.test`,
          emailVerified: true,
        };
        await control
          .prepare(
            "INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,1,1)",
          )
          .bind(u.id, u.name, u.email)
          .run();
        return u;
      }),
    );
    await control.batch(
      ["testbeta1", "testbeta2"].map((id) =>
        control
          .prepare(
            "INSERT INTO beta_slots(holding_id,created_at,updated_at) VALUES(?,1,1)",
          )
          .bind(id),
      ),
    );
    const inputs = users.map((_, i) => ({
      companyName: `Company ${i}`,
      branchName: "First branch",
      slug: `company-${key().slice(0, 8)}`,
    }));
    await expect(
      onboard(
        {
          ...betaEnv,
          TEST_ONE_WRITE: {
            prepare: env.H001_READ_DB.prepare.bind(env.H001_READ_DB),
            batch: async () => {
              throw new Error("Injected provisioning failure");
            },
          },
        },
        users[0],
        inputs[0],
      ),
    ).rejects.toThrow("Injected provisioning failure");
    expect(
      await control
        .prepare("SELECT state FROM beta_slots WHERE owner_user_id=?")
        .bind(users[0].id)
        .first("state"),
    ).toBe("provisioning");
    const results = await Promise.all(
      users.map((u, i) => onboard(betaEnv, u, inputs[i])),
    );
    expect(results.filter((x) => x.state === "active")).toHaveLength(2);
    expect(results.filter((x) => x.state === "waitlisted")).toHaveLength(1);
    const activeSlots = (
      await control
        .prepare("SELECT holding_id,owner_user_id FROM beta_slots WHERE state='active' ORDER BY holding_id")
        .all<{ holding_id: string; owner_user_id: string }>()
    ).results;
    for (const activeSlot of activeSlots) {
      const holding = registeredResources(betaEnv, activeSlot.holding_id).write;
      const companyId = `company-${activeSlot.holding_id}`;
      expect(
        await holding
          .prepare("SELECT status,version FROM company_provisioning WHERE company_id=? AND holding_id=?")
          .bind(companyId, activeSlot.holding_id)
          .first(),
      ).toEqual({ status: "active", version: 2 });
      expect(
        await holding
          .prepare("SELECT count(*) count FROM staff_levels WHERE company_id=? AND active=1 AND level_number BETWEEN 1 AND 6")
          .bind(companyId)
          .first("count"),
      ).toBe(6);
      expect(
        await holding
          .prepare("SELECT count(*) count FROM organization_positions WHERE company_id=? AND active=1 AND is_ceo=1")
          .bind(companyId)
          .first("count"),
      ).toBe(1);
      expect(
        await holding
          .prepare("SELECT level_id,position_id,job_title FROM hr_engagements WHERE company_id=? AND id=?")
          .bind(companyId, `engagement-${activeSlot.owner_user_id}`)
          .first(),
      ).toMatchObject({
        level_id: `level-${companyId}`,
        position_id: null,
        job_title: "Team member",
      });
      expect(
        (
          await holding
            .prepare("SELECT action FROM company_provisioning_events WHERE company_id=? ORDER BY created_at,id")
            .bind(companyId)
            .all<{ action: string }>()
        ).results.map((event) => event.action),
      ).toEqual(["started", "completed"]);
    }
    const accepted = results.findIndex((x) => x.state === "active");
    await expect(
      onboard(betaEnv, users[accepted], inputs[accepted]),
    ).resolves.toMatchObject(results[accepted]);
    expect(
      await control
        .prepare("SELECT count(*) n FROM beta_slots WHERE state='active'")
        .first("n"),
    ).toBe(2);
    await expect(
      onboard(betaEnv, { ...users[0], emailVerified: false }, inputs[0]),
    ).rejects.toMatchObject({ code: "VERIFY_EMAIL" });
  });
  it("links only the invited verified account and retries without duplicate access", async () => {
    const user = {
        id: key(),
        name: "Invited person",
        email: `${key()}@example.test`,
        emailVerified: true,
      },
      token = key();
    await inviteMember(
      db,
      owner,
      { email: user.email, branchId: branch },
      key(),
      token,
    );
    await expect(
      acceptMember(db, { ...user, email: "someone-else@example.test" }, token),
    ).rejects.toMatchObject({ status: 404 });
    await expect(
      acceptMember(db, { ...user, emailVerified: false }, token),
    ).rejects.toMatchObject({ code: "VERIFY_EMAIL" });
    await expect(acceptMember(db,user,token)).rejects.toMatchObject({code:'INVITATION_REVIEW_REQUIRED'});
    await approveInvitation();
    await acceptMember(db, user, token);
    await acceptMember(db, user, token);
    expect(
      await db
        .prepare("SELECT count(*) n FROM assignments WHERE user_id=?")
        .bind(user.id)
        .first("n"),
    ).toBe(1);
  });
  it("validates registry capacity and refuses arbitrary binding names", () => {
    expect(resourceSlots({ ERPX_RESOURCE_SLOTS: registry })).toHaveLength(2);
    expect(() => registeredDatabases(env, "unexpected")).toThrow(
      "HOLDING_NOT_FOUND",
    );
    expect(() =>
      resourceSlots({
        ERPX_RESOURCE_SLOTS: JSON.stringify(
          Array.from({ length: 21 }, (_, i) => ({
            id: `beta${i}`,
            write: `W${i}_DB`,
            read: `R${i}_DB`,
            files: `F${i}_FILES`,
            beta: true,
          })),
        ),
      }),
    ).toThrow("BETA_CAPACITY_EXCEEDED");
  });
  it("accepts an owner-issued IT invitation alongside ordinary employee access", async () => {
    const user = {
        id: key(),
        name: "Invited IT administrator",
        email: `${key()}@example.test`,
        emailVerified: true,
      },
      token = key();
    await inviteMember(
      db,
      owner,
      { email: user.email, branchId: branch, role: "it_admin" },
      key(),
      token,
    );
    await expect(acceptMember(db,user,token)).rejects.toMatchObject({code:'INVITATION_REVIEW_REQUIRED'});
    await approveInvitation();
    await acceptMember(db, user, token);
    await acceptMember(db, user, token);
    const assignments = (
      await db
        .prepare(
          "SELECT role,scope_kind,company_id FROM assignments WHERE user_id=? ORDER BY role",
        )
        .bind(user.id)
        .all()
    ).results;
    expect(assignments).toHaveLength(2);
    expect(assignments.find((a) => a.role === "it_admin")).toEqual({
      role: "it_admin",
      scope_kind: "holding",
      company_id: company,
    });
    expect(assignments.some((a) => a.role === "employee")).toBe(true);
  });
});

it("paginates tied timestamps without dropping or duplicating conversations", async () => {
  const timestamp = Date.now() - 100;
  await db.batch(
    Array.from({ length: 22 }, (_, i) =>
      db
        .prepare(
          "INSERT INTO social_posts(id,kind,author_id,author_name,company_id,body,last_command_id,created_at,updated_at) VALUES(?,'team',?,?,?,'Update',?,?,?)",
        )
        .bind(
          `paging-${company}-${String(i).padStart(2, "0")}`,
          owner.userId,
          owner.name,
          company,
          key(),
          timestamp,
          timestamp,
        ),
    ),
  );
  const first = await homePosts(db, owner),
    last = first.at(-1)!;
  const next = await homePosts(db, owner, {
    before: last.created_at,
    beforeId: last.id,
  });
  expect(first).toHaveLength(20);
  expect(next).toHaveLength(2);
  expect(new Set([...first, ...next].map((p) => p.id)).size).toBe(22);
});
it("enforces member and branch capacity before granting access", async () => {
  await db
    .prepare(
      "UPDATE beta_policy SET member_limit=3,branch_limit=1 WHERE company_id=?",
    )
    .bind(company)
    .run();
  await expect(
    inviteMember(
      db,
      owner,
      { email: "full@example.test", branchId: branch },
      key(),
      key(),
    ),
  ).rejects.toMatchObject({ status: 409 });
  await expect(
    addBetaBranch(db, owner, "Extra branch", key()),
  ).rejects.toMatchObject({ status: 409 });
});

it("lets the original reporter track status without granting task or evidence access", async () => {
  const p = await post();
  await saveTask(db, worker, null, task(), key(), false, await source(p.id));
  const detail = await postView(db, owner, p.id);
  expect(detail.trackingState).toBe("draft");
  expect(detail.task).toBeNull();
  await expect(
    taskDetail(db, owner, `task-post-${p.id}`),
  ).rejects.toMatchObject({ status: 404 });
});

const assistantMention: Mention = {kind:"agent",targetId:"erpx-assistant",label:"ERPX assistant",start:0,end:15};
const replyEnv={...env,AI_BASE_URL:"https://provider.example/v1",AI_API_KEY:"test",AI_MODEL:"test"};
async function assistantPost() {
 const draft=await savePostDraft(db,owner,null,{body:"@ERPX assistant Help #ทีมงาน",mentions:[assistantMention],branchId:branch,expectedVersion:1},key());
 expect(await db.prepare("SELECT count(*) n FROM social_agent_replies WHERE owner_id=?").bind(draft.id).first<number>("n")).toBe(0);
 const p=await publishPost(db,owner,draft.id,draft.version,key());
 return {...p,draft};
}
describe("inline mentions, hashtags and assistant replies",()=>{
 it("preserves draft metadata, indexes hashtags and queues only once",async()=>{
  const p=await assistantPost();
  await publishPost(db,owner,p.draft.id,p.draft.version,key());
  const view=await postView(db,owner,p.id);
  expect(view.mentions).toEqual([assistantMention]);expect(view.agentReplies).toHaveLength(1);
  expect((await homePosts(db,owner,{tag:"ทีมงาน"})).map(p=>p.id)).toContain(p.id);
  expect(await textSuggestions(db,owner,"#","ทีม")).toContainEqual({kind:"tag",targetId:"ทีมงาน",label:"ทีมงาน",detail:"Hashtag"});
  await postAction(db,owner,p.id,{action:"comment",body:"Another #release"},key());
  expect((await homePosts(db,owner,{tag:"release"})).map(p=>p.id)).toContain(p.id);
  await postAction(db,owner,p.id,{action:"hide"},key());
  expect(await textSuggestions(db,owner,"#","ทีม")).toEqual([]);
 });
 it("rejects forged targets and stale offsets",async()=>{
  await expect(savePostDraft(db,owner,null,{body:"@ERPX assistant",mentions:[{...assistantMention,targetId:"fake"}],branchId:branch,expectedVersion:1},key())).rejects.toMatchObject({code:"MENTION_UNAVAILABLE"});
  await expect(savePostDraft(db,owner,null,{body:"edited",mentions:[assistantMention],branchId:branch,expectedVersion:1},key())).rejects.toMatchObject({code:"MENTION_CHANGED"});
 });
 it("publishes one attributed AI reply, accounts usage, and never recursively queues",async()=>{
  const p=await assistantPost();
  const provider=vi.fn(async()=>({result:{body:"Here is some help. @ERPX assistant"},inputTokens:10,outputTokens:8}));
  await Promise.all([drainSocialAgent(db,replyEnv,p.id,provider),drainSocialAgent(db,replyEnv,p.id,provider)]);
  await drainSocialAgent(db,replyEnv,p.id,provider);
  const view=await postView(db,owner,p.id);
  expect(provider).toHaveBeenCalledTimes(1);expect(view.comments).toHaveLength(1);
  expect(view.comments[0].author_kind).toBe("agent");expect(view.agentReplies[0].state).toBe("completed");
  expect(view.agentReplies[0].requester_name).toBe(owner.name);
  expect(await db.prepare("SELECT output_tokens FROM social_ai_usage WHERE id=?").bind(`${view.agentReplies[0].id}:1`).first<number>("output_tokens")).toBe(8);
 });
 it("keeps the post when AI fails and lets only its requester retry",async()=>{
  const p=await assistantPost();
  await drainSocialAgent(db,replyEnv,p.id,async()=>{throw new Error("offline");});
  let view=await postView(db,owner,p.id);expect(view.agentReplies[0].state).toBe("failed");
  await expect(retryAgentReply(db,worker,view.agentReplies[0].id,key())).rejects.toMatchObject({code:"REPLY_UNAVAILABLE"});
  const k=key();await retryAgentReply(db,owner,view.agentReplies[0].id,k);await retryAgentReply(db,owner,view.agentReplies[0].id,k);
  await drainSocialAgent(db,replyEnv,p.id,async()=>({result:{body:"Ready"},inputTokens:1,outputTokens:1}));
  view=await postView(db,owner,p.id);expect(view.comments).toHaveLength(1);expect(view.agentReplies[0].state).toBe("completed");
  expect((await db.prepare("SELECT error_code FROM social_agent_replies WHERE id=?").bind(view.agentReplies[0].id).first<{error_code:string|null}>())?.error_code).toBeNull();
 });
 it("shows safe provider guidance when AI authentication is rejected",async()=>{
  const p=await assistantPost();
  await drainSocialAgent(db,replyEnv,p.id,async()=>{throw new ApiError(503,"AI_PROVIDER_ERROR","The provider rejected authentication. Check the API key.");});
  const view=await postView(db,owner,p.id);
  expect(view.agentReplies[0].state).toBe("failed");
  expect(view.agentReplies[0].error).toBe("The provider rejected authentication. Check the API key.");
  expect(view.agentReplies[0].error_code).toBe("AI_PROVIDER_ERROR");
 });
 it("blocks exhausted quotas and revoked access without publishing",async()=>{
  const p=await assistantPost();const provider=vi.fn(async()=>({result:{body:"Ready"},inputTokens:1,outputTokens:1}));
  await db.prepare("UPDATE beta_policy SET ai_monthly_limit=0 WHERE company_id=?").bind(company).run();
  await drainSocialAgent(db,replyEnv,p.id,provider);expect(provider).not.toHaveBeenCalled();
  const view=await postView(db,owner,p.id);expect(view.agentReplies[0].state).toBe("failed");
  await db.prepare("UPDATE beta_policy SET ai_monthly_limit=100 WHERE company_id=?").bind(company).run();
  await retryAgentReply(db,owner,view.agentReplies[0].id,key());
  await drainSocialAgent(db,replyEnv,p.id,async()=>{await db.prepare("UPDATE assignments SET revoked_at=? WHERE user_id=?").bind(Date.now(),owner.userId).run();return {result:{body:"Do not publish"},inputTokens:1,outputTokens:1};});
  expect((await postView(db,worker,p.id)).comments).toHaveLength(0);
 });
 it("invokes the assistant from a comment and fails expired leases visibly",async()=>{
  const p=await post();await postAction(db,owner,p.id,{action:"comment",body:"@ERPX assistant Help",mentions:[assistantMention]},key());
  const view=await postView(db,owner,p.id);expect(view.agentReplies).toHaveLength(1);
  await db.prepare("UPDATE social_agent_replies SET state='running',lease_until=1 WHERE post_id=?").bind(p.id).run();
  const provider=vi.fn();await drainSocialAgent(db,replyEnv,p.id,provider);expect(provider).not.toHaveBeenCalled();
  expect((await postView(db,owner,p.id)).agentReplies[0].state).toBe("failed");
 });
 it("queues one AI answer for an idempotently replayed Thai reply",async()=>{
  const p=await post(),parentKey=key(),sourceKey=key(),replyKey=key();
  await postAction(db,owner,p.id,{action:"comment",body:"Please inspect this item."},parentKey);
  const parentId=`comment-${parentKey}`;
  await postAction(db,worker,p.id,{action:"comment",body:"I am checking it now.",parentCommentId:parentId},sourceKey);
  const sourceId=`comment-${sourceKey}`;
  const mention:Mention={kind:"agent",targetId:"erpx-assistant",label:"AI",start:0,end:3};
  expect(await textSuggestions(db,owner,"@","AI")).toContainEqual({kind:"agent",targetId:"erpx-assistant",label:"AI",detail:"AI · replies after publishing"});
  const input={action:"comment" as const,body:"@AI ช่วยตรวจสอบรายการนี้ให้หน่อย",mentions:[mention],parentCommentId:sourceId};
  const first=await postAction(db,owner,p.id,input,replyKey);
  expect(await postAction(db,owner,p.id,input,replyKey)).toEqual(first);
  const ownerId=`comment-${replyKey}`;
  const queued=await db.prepare("SELECT id,owner_type,owner_id,requester_name FROM social_agent_replies WHERE post_id=? AND owner_id=?")
    .bind(p.id,ownerId).all<{id:string;owner_type:string;owner_id:string;requester_name:string|null}>();
  expect(queued.results).toHaveLength(1);
  expect(queued.results[0]).toMatchObject({id:`agent-${ownerId}`,owner_type:"comment",owner_id:ownerId,requester_name:owner.name});
  const provider=vi.fn(async()=>({result:{body:"ตรวจสอบแล้ว เรียบร้อย",},inputTokens:4,outputTokens:3}));
  await Promise.all([drainSocialAgent(db,replyEnv,p.id,provider),drainSocialAgent(db,replyEnv,p.id,provider)]);
  await drainSocialAgent(db,replyEnv,p.id,provider);
  const view=await postView(db,owner,p.id);
  expect(provider).toHaveBeenCalledTimes(1);
  expect(view.comments.find(comment=>comment.id===ownerId)).toMatchObject({parent_comment_id:sourceId,mentions:[mention]});
  expect(view.comments.find(comment=>comment.id===`reply-agent-${ownerId}`)).toMatchObject({author_kind:"agent",parent_comment_id:ownerId});
  expect(view.agentReplies.filter(reply=>reply.owner_type==="comment"&&reply.owner_id===ownerId)).toHaveLength(1);
 });
});

it("scopes people/record suggestions and notifies only readable mentioned people",async()=>{
 for(const actor of [owner,worker,reviewer]) await control.batch([
  control.prepare('INSERT INTO "user"(id,name,email,"createdAt","updatedAt") VALUES(?,?,?,1,1) ON CONFLICT(id) DO NOTHING').bind(actor.userId,actor.name,actor.email),
  control.prepare("INSERT INTO principals(id,user_id,display_name,created_at) VALUES(?,?,?,1)").bind(actor.userId,actor.userId,actor.name),
 ]);
 // Reviewer belongs to another branch and must not receive a branch-post notification.
 const other=`other-${key()}`;await db.batch(branchStatements(db,company,other,"Other",other));
 await db.prepare("UPDATE assignments SET scope_node_id=? WHERE user_id=?").bind(other,reviewer.userId).run();
 owner.modules=["brand"];
 const rec=`rec-${key()}`;
 await db.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,fields_json,company_id,branch_id,created_by,updated_by,created_at,updated_at) VALUES('brand',?,'drafts','Launch','[]',?,?,?,?,1,1)").bind(rec,company,branch,owner.userId,owner.userId).run();
 const suggestions=await textSuggestions(db,owner,"@","");
 expect(suggestions.some(s=>s.targetId===worker.userId)).toBe(true);
 expect(suggestions.some(s=>s.targetId===`brand/${rec}`)).toBe(true);
 const body="@Worker @Reviewer @Launch";
 const mentions:Mention[]=[{kind:"person",targetId:worker.userId,label:"Worker",start:0,end:7},{kind:"person",targetId:reviewer.userId,label:"Reviewer",start:8,end:17},{kind:"record",targetId:`brand/${rec}`,label:"Launch",start:18,end:25}];
 const d=await savePostDraft(db,owner,null,{body,mentions,branchId:branch,expectedVersion:1},key());
 const p=await publishPost(db,owner,d.id,d.version,key());
 const notified=(await db.prepare("SELECT user_id FROM staff_notifications WHERE record_id=?").bind(p.id).all<{user_id:string}>()).results;
 expect(notified.map(n=>n.user_id)).toEqual([worker.userId]);
 await expect(savePostDraft(db,{...owner,modules:[]},null,{body,mentions,branchId:branch,expectedVersion:1},key())).rejects.toMatchObject({code:"MODULE_FORBIDDEN"});
 await db.prepare("UPDATE assignments SET revoked_at=1 WHERE user_id=?").bind(worker.userId).run();
 expect((await textSuggestions(db,owner,"@","Worker")).some(s=>s.kind==="person")).toBe(false);
});

it("keeps the newest replies visible in long conversations",async()=>{
 const p=await post(),now=Date.now();
 await db.batch(Array.from({length:55},(_,i)=>db.prepare("INSERT INTO social_comments(id,post_id,author_id,author_name,body,created_at,updated_at) VALUES(?,?,?,'Owner',?,?,?)").bind(`${p.id}-${i}`,p.id,owner.userId,`Reply ${i}`,now+i,now+i)));
 const view=await postView(db,owner,p.id);
 expect(view.comments).toHaveLength(20);expect(view.comments[0].body).toBe("Reply 35");expect(view.comments.at(-1)?.body).toBe("Reply 54");
 expect(view.commentCount).toBe(55);
 const ids:string[]=[];let cursor:string|undefined;
 do {const page=await postComments(db,owner,p.id,cursor);ids.push(...page.items.map(c=>c.id));cursor=page.nextCursor??undefined;} while(cursor);
 expect(ids).toHaveLength(55);expect(new Set(ids).size).toBe(55);
 const feed=(await homePosts(db,owner)).find(row=>row.id===p.id)!;
 expect(feed.comments).toHaveLength(2);expect(feed.commentCount).toBe(55);
 await expect(postComments(db,{...owner,companyId:'another-company'},p.id)).rejects.toMatchObject({code:'POST_NOT_FOUND'});
});
