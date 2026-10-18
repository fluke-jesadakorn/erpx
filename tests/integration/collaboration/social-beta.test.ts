import { sealSecret } from "../../../src/server/modules/identity_access/public/iam-mfa";
import { loadIamAuthority } from "../../../src/server/modules/identity_access/public/iam-store";
import { selectWorkRole } from "../../../src/server/modules/work_projection/public/work-role-store";
import { decideIamReview,validateIamGrant,grantInput } from "../../../src/server/modules/identity_access/public/iam-service";
import { iamActorFromWork } from "../../../src/server/modules/identity_access/public/iam-context";
import { drainSocialAgent, retryAgentReply } from "../../../src/server/modules/collaboration/public/social-agent";
import { ApiError } from "../../../src/server/modules/runtime/adapters/http";
import { textSuggestions } from "../../../src/server/modules/collaboration/public/social-text";
import type { Mention } from "../../../src/server/modules/collaboration/public/rules/social-text";
import { plainRichContent, type RichNode } from "../../../src/lib/shared/home-rich-content";
import { env } from "../../support/runtime-env";
import { applyMigrations, type TestMigration } from "../../support/runtime-env";
import { afterEach, beforeAll, beforeEach, describe, it, expect, vi } from "vitest";
import {
  onboard,
  memberStatements,
  branchStatements,
} from "../../../src/server/modules/manage_erpx/public/beta-onboarding";
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
} from "../../../src/server/modules/collaboration/public/social-home";
import { sourceSnapshot, readablePost } from "../../../src/server/modules/collaboration/public/social-source";
import { createAiDraft, aiResultSchema, callAi } from "../../../src/server/modules/collaboration/public/social-ai";
import {
  saveTask,
  taskSchema,
  taskDetail,
  transitionTask,
} from "../../../src/server/modules/operations/public/staff-tasks";
import { betaDestinationAllowed } from "../../../src/server/modules/identity_access/public/beta-access";
import {
  addBetaBranch,
  inviteMember,
  acceptMember,
} from "../../../src/server/modules/manage_erpx/public/beta-team";
import {
  resourceSlots,
  registeredDatabases,
  registeredResources,
} from "../../../src/lib/runtime/holding-registry";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({ runtimeEnv: async () => env, holdingResources: async () => ({write:env.H001_DB,read:env.H001_READ_DB,files:env.H001_FILES}) }));
const db = env.H001_DB,
  control = env.CONTROL_DB,
  key = () => crypto.randomUUID();
afterEach(()=>vi.unstubAllEnvs());
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
 const now=Date.now(),sessionId=key(),assignmentVersion=await db.prepare('SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=?').bind(actor.assignmentId,actor.userId,actor.companyId).first<number>('version');if(assignmentVersion===null)throw Error('Actual fixture assignment is required');
 await control.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?) ON CONFLICT DO NOTHING').bind(actor.userId,actor.name,actor.email,now,now).run();
 await control.prepare('INSERT INTO session(id,"expiresAt",token,"createdAt","updatedAt","userId") VALUES(?,?,?,?,?,?)').bind(sessionId,new Date(now+86400000).toISOString(),key(),new Date().toISOString(),new Date().toISOString(),actor.userId).run();
 await control.prepare('INSERT INTO iam_mfa(user_id,secret,enabled) VALUES(?,?,true) ON CONFLICT DO NOTHING').bind(actor.userId,await sealSecret('JBSWY3DPEHPK3PXP')).run();
 await control.prepare('INSERT INTO iam_mfa_sessions VALUES(?,?,?)').bind(sessionId,actor.userId,now).run();
 await selectWorkRole(db,{sessionId,userId:actor.userId,companyId:actor.companyId,assignmentId:actor.assignmentId,assignmentVersion,revision:0,expiresAt:now+86400000});
 actor.authority={sessionId,assignmentVersion,revision:1};actor.iam=await loadIamAuthority(db,actor.companyId,actor.userId,actor.role);return actor;
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
describe('governed Social engagement',()=>{
 it('retains author comment corrections and replies while independently reported posts are moderated with replay-safe evidence',async()=>{
  const p=await post(),commentKey=key(),replyKey=key();await postAction(db,worker,p.id,{action:'comment',body:'Correct this fictional incident detail'},commentKey);
  const commentId=`comment-${commentKey}`;await postAction(db,reviewer,p.id,{action:'comment',body:'Independent follow-up evidence',parentCommentId:commentId},replyKey);
  await expect(postAction(db,reviewer,p.id,{action:'edit_comment',commentId,body:'Forged author correction',richContent:plainRichContent('Forged author correction')},key())).rejects.toMatchObject({code:'COMMENT_AUTHOR_REQUIRED'});
  await postAction(db,worker,p.id,{action:'edit_comment',commentId,body:'Corrected fictional incident detail',richContent:plainRichContent('Corrected fictional incident detail')},key());expect((await postComments(db,worker,p.id)).items.find(row=>row.id===commentId)?.body).toBe('Corrected fictional incident detail');
  const deletion=key();await postAction(db,worker,p.id,{action:'delete_comment',commentId},deletion);await postAction(db,worker,p.id,{action:'delete_comment',commentId},deletion);
  const comments=await postComments(db,reviewer,p.id);expect(comments.items.some(row=>row.id===commentId)).toBe(false);expect(comments.items.find(row=>row.id===`comment-${replyKey}`)).toMatchObject({body:'Independent follow-up evidence',parent_comment_id:commentId});
  const reportKey=key();await postAction(db,reviewer,p.id,{action:'report',body:'Synthetic moderation policy concern'},reportKey);await postAction(db,reviewer,p.id,{action:'report',body:'Synthetic moderation policy concern'},reportKey);expect(await db.prepare('SELECT count(*) n FROM social_reports WHERE post_id=? AND reporter_id=?').bind(p.id,reviewer.userId).first('n')).toBe(1);
  await expect(postAction(db,reviewer,p.id,{action:'hide'},key())).rejects.toMatchObject({code:'COMPANY_AUTHORITY_REQUIRED'});
  const moderation=key();await postAction(db,owner,p.id,{action:'hide'},moderation);await postAction(db,owner,p.id,{action:'hide'},moderation);
  await expect(postView(db,worker,p.id)).rejects.toMatchObject({code:'POST_NOT_FOUND'});expect((await homePosts(db,worker)).some(row=>row.id===p.id)).toBe(false);
  expect(await db.prepare("SELECT count(*) n FROM audit_events WHERE record_id=? AND action='social.post.hide'").bind(p.id).first('n')).toBe(1);
  await expect(postAction(db,reviewer,p.id,{action:'report',body:'Synthetic moderation policy concern'},reportKey)).rejects.toMatchObject({code:'POST_NOT_FOUND'});
 });
 it('keeps appreciation and following as unique non-decision signals across races, retries and current-source withdrawal',async()=>{
  const p=await post(),appreciation=key(),following=key();await postAction(db,worker,p.id,{action:'appreciate',active:true},appreciation);await postAction(db,worker,p.id,{action:'appreciate',active:true},appreciation);
  await Promise.all([1,2].map(()=>postAction(db,worker,p.id,{action:'appreciate',active:true},key())));await Promise.all([1,2].map(()=>postAction(db,worker,p.id,{action:'follow',active:true},key())));await postAction(db,worker,p.id,{action:'follow',active:true},following);
  const view=await postView(db,worker,p.id);expect(view.appreciationCount).toBe(1);expect(Boolean(view.appreciated)).toBe(true);expect(Boolean(view.followed)).toBe(true);expect(view.version).toBe(p.version);expect((await homePosts(db,worker,{following:true})).map(row=>row.id)).toContain(p.id);
  await postAction(db,worker,p.id,{action:'appreciate',active:false},key());await postAction(db,worker,p.id,{action:'follow',active:false},key());const cleared=await postView(db,worker,p.id);expect(cleared.appreciationCount).toBe(0);expect(Boolean(cleared.followed)).toBe(false);expect((await homePosts(db,worker,{following:true})).some(row=>row.id===p.id)).toBe(false);
  await postAction(db,owner,p.id,{action:'hide'},key());await expect(postAction(db,worker,p.id,{action:'appreciate',active:true},appreciation)).rejects.toMatchObject({code:'POST_NOT_FOUND'});await expect(postAction(db,worker,p.id,{action:'follow',active:true},following)).rejects.toMatchObject({code:'POST_NOT_FOUND'});
  expect(await db.prepare('SELECT count(*) n FROM social_reactions WHERE post_id=? AND principal_id=?').bind(p.id,worker.userId).first('n')).toBe(0);
  expect(await db.prepare("SELECT count(*) n FROM approval_instances WHERE record_type='social' AND record_id=?").bind(p.id).first('n')).toBe(0);
 });
 it('filters hashtag feeds and suggestions by live audience membership and hides their content after revocation',async()=>{
  const body=`Private release #governed${key().replaceAll('-','')}`,tag=body.split('#')[1],draft=await savePostDraft(db,owner,null,{body,audience:{kind:'participants',userIds:[worker.userId]},expectedVersion:1},key()),p=await publishPost(db,owner,draft.id,draft.version,key());
  expect((await homePosts(db,worker,{tag})).map(row=>row.id)).toContain(p.id);expect(await textSuggestions(db,worker,'#',tag)).toContainEqual({kind:'tag',targetId:tag,label:tag,detail:'Hashtag'});
  expect((await homePosts(db,reviewer,{tag}))).toEqual([]);expect(await textSuggestions(db,reviewer,'#',tag)).toEqual([]);await expect(postView(db,reviewer,p.id)).rejects.toMatchObject({code:'POST_NOT_FOUND'});
  const follow=key();await postAction(db,worker,p.id,{action:'follow',active:true},follow);await db.prepare('UPDATE assignments SET revoked_at=? WHERE company_id=? AND user_id=?').bind(Date.now(),company,worker.userId).run();
  expect(await homePosts(db,worker,{tag,following:true})).toEqual([]);expect(await textSuggestions(db,worker,'#',tag)).toEqual([]);await expect(postComments(db,worker,p.id)).rejects.toMatchObject({code:'POST_NOT_FOUND'});await expect(postAction(db,worker,p.id,{action:'follow',active:true},follow)).rejects.toMatchObject({code:'POST_NOT_FOUND'});
  expect(JSON.stringify(await homePosts(db,reviewer,{tag}))).not.toContain(body);
 });
});
async function readySocialImage(draftId: string, name: string,actual?:{checksum:string;bytes:number;actorId?:string}) {
  const uploadId = key(), attachmentId = key(), now = Date.now(),checksum=actual?.checksum??"test-checksum",bytes=actual?.bytes??1;
  const objectKey = `test/social/${uploadId}`;
  await db.batch([
    db.prepare(`INSERT INTO upload_intents
      (id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,revision,status,created_by,expires_at,created_at,updated_at)
      VALUES(?,'social_draft',?,'post-image',?,'image/png',?,?,?,?,?,1,'ready',?,?,?,?)`)
      .bind(uploadId,draftId,name,bytes,bytes,checksum,`${objectKey}/staging`,objectKey,actual?.actorId??owner.userId,now+900_000,now,now),
    db.prepare(`INSERT INTO attachments
      (id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at)
      VALUES(?,?,'social_draft',?,'post-image',?,1,?,'image/png',?,?,?,?)`)
      .bind(attachmentId,uploadId,draftId,objectKey,name,bytes,checksum,actual?.actorId??owner.userId,now),
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
    expect(await homePosts(db, reviewer, { teamId: team })).toEqual([]);
    await db.prepare("UPDATE assignments SET scope_node_id=? WHERE id=?").bind(branch, reviewer.assignmentId).run();
    const draft = await savePostDraft(db, worker, null, { body: "Team note", audience: { kind: "team", id: team }, expectedVersion: 1 }, key());
    const published = await publishPost(db, worker, draft.id, draft.version, key());
    const teamFeed = await homePosts(db, worker, { teamId: team });
    expect(teamFeed.some(post => post.id === published.id)).toBe(true);
    expect(teamFeed.every(post => post.audience_kind === "team" && post.audience_id === team)).toBe(true);
    await db.prepare("DELETE FROM messaging_team_members WHERE team_id=? AND user_id=?").bind(team, reviewer.userId).run();
    await expect(readablePost(db, reviewer, published.id)).rejects.toMatchObject({ code: "POST_NOT_FOUND" });
    expect(await homePosts(db, reviewer, { teamId: team })).toEqual([]);
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
      : { type: "text" as const, doc: (plainRichContent(block.text).blocks[0] as {type:"text";doc:RichNode}).doc }) });
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
      { action: "submit", expectedVersion: 1, note: "", attachmentSetIds:[] },
      key(),
    );
    await transitionTask(
      db,
      worker,
      id,
      { action: "start", expectedVersion: 2, note: "", attachmentSetIds:[] },
      key(),
    );
    await transitionTask(
      db,
      worker,
      id,
      {
        action: "complete",
        expectedVersion: 3,
        note: "Replaced the faulty switch. Tested.",attachmentSetIds:[],
      },
      key(),
    );
    await expect(
      transitionTask(
        db,
        worker,
        id,
        { action: "approve", expectedVersion: 4, note: "My own work",attachmentSetIds:[] },
        key(),
      ),
    ).rejects.toMatchObject({ status: 403 });
    await transitionTask(
      db,
      reviewer,
      id,
      { action: "approve", expectedVersion: 4, note: "Inspected and accepted",attachmentSetIds:[] },
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
        vi.stubGlobal("fetch",vi.fn(
          async () => new Response(JSON.stringify(payload), { status: 200 }),
        ));
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
    const failedProvisioningEnv={
          ...betaEnv,
          TEST_ONE_WRITE: {
            prepare: env.H001_READ_DB.prepare.bind(env.H001_READ_DB),
            batch: async () => {
              throw new Error("Injected provisioning failure");
            },
          },
        };
    await expect(
      onboard(
        failedProvisioningEnv,
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
    expect(() => registeredDatabases({ERPX_RESOURCE_SLOTS:undefined,...env}, "unexpected")).toThrow(
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


it("rolls back a beta branch and department on late outbox failure, then retries and replays the same key", async () => {
  const commandKey = key(), branchId = `branch-${commandKey}`, name = "Atomic company branch";
  const suffix = key().replace(/[^a-zA-Z0-9]/g, ""), fault = `beta_branch_fault_${suffix}`, trigger = `beta_branch_outbox_${suffix}`;
  await db.prepare(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='${commandKey}' THEN RAISE EXCEPTION 'BETA_BRANCH_LATE_FAILURE'; END IF; RETURN NEW; END $$`).run();
  await db.prepare(`CREATE TRIGGER ${trigger} AFTER INSERT ON outbox_events FOR EACH ROW EXECUTE FUNCTION ${fault}()`).run();
  try {
    await expect(addBetaBranch(db, owner, name, commandKey)).rejects.toThrow("BETA_BRANCH_LATE_FAILURE");
    expect(await db.prepare("SELECT count(*)::int count FROM organization_nodes WHERE id=? OR parent_id=?").bind(branchId, branchId).first("count")).toBe(0);
    for (const table of ["command_receipts", "audit_events", "outbox_events"]) {
      expect(await db.prepare(`SELECT count(*)::int count FROM ${table} WHERE command_id=?`).bind(commandKey).first("count")).toBe(0);
    }
  } finally {
    await db.prepare(`DROP TRIGGER ${trigger} ON outbox_events`).run();
    await db.prepare(`DROP FUNCTION ${fault}()`).run();
  }
  const result = await addBetaBranch(db, owner, name, commandKey);
  expect(await addBetaBranch(db, owner, name, commandKey)).toEqual(result);
  expect(await db.prepare("SELECT count(*)::int count FROM organization_nodes WHERE id=? OR parent_id=?").bind(branchId, branchId).first("count")).toBe(2);
  for (const table of ["command_receipts", "audit_events", "outbox_events"]) {
    expect(await db.prepare(`SELECT count(*)::int count FROM ${table} WHERE command_id=?`).bind(commandKey).first("count")).toBe(1);
  }
});

it("restores post content, revisions and private image state on late outbox failure, then retries and replays once", async () => {
  const content: PostContent = [
    {type:"text",text:"Original evidence"},
    {type:"image",slotId:key(),attachmentId:null,name:"removed.png"},
    {type:"image",slotId:key(),attachmentId:null,name:"retained.png"},
  ];
  const draft=await savePostDraft(db,owner,null,{body:"Original evidence",content,branchId:branch,expectedVersion:1},key());
  const removed=await readySocialImage(draft.id,"removed.png"),retained=await readySocialImage(draft.id,"retained.png");
  const populated:PostContent=[content[0],{...content[1],type:"image",slotId:(content[1] as Extract<PostContent[number],{type:"image"}>).slotId,attachmentId:removed.attachmentId,name:"removed.png"},{...content[2],type:"image",slotId:(content[2] as Extract<PostContent[number],{type:"image"}>).slotId,attachmentId:retained.attachmentId,name:"retained.png"}];
  const saved=await savePostDraft(db,owner,draft.id,{body:"Original evidence",content:populated,branchId:branch,expectedVersion:draft.version},key());
  const published=await publishPost(db,owner,draft.id,saved.version,key()),commandKey=key();
  const input={action:"edit" as const,body:"Reviewed evidence",content:[{type:"text" as const,text:"Reviewed evidence"},populated[2]],expectedVersion:published.version};
  const snapshots=async()=>Promise.all([
    db.prepare("SELECT * FROM social_posts WHERE id=?").bind(published.id).first(),
    db.prepare("SELECT * FROM social_post_revisions WHERE post_id=? ORDER BY id").bind(published.id).all().then(rows=>rows.results),
    db.prepare("SELECT * FROM attachments WHERE owner_type='social_draft' AND owner_id=? ORDER BY id").bind(draft.id).all().then(rows=>rows.results),
    db.prepare("SELECT * FROM upload_intents WHERE owner_type='social_draft' AND owner_id=? ORDER BY id").bind(draft.id).all().then(rows=>rows.results),
  ]);
  const before=await snapshots(),suffix=key().replace(/[^a-zA-Z0-9]/g,''),fault=`social_edit_fault_${suffix}`,trigger=`social_edit_outbox_${suffix}`;
  await db.prepare(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='${commandKey}' THEN RAISE EXCEPTION 'SOCIAL_EDIT_LATE_FAILURE'; END IF; RETURN NEW; END $$`).run();
  await db.prepare(`CREATE TRIGGER ${trigger} AFTER INSERT ON outbox_events FOR EACH ROW EXECUTE FUNCTION ${fault}()`).run();
  try {
    await expect(postAction(db,owner,published.id,input,commandKey)).rejects.toThrow('SOCIAL_EDIT_LATE_FAILURE');
    expect(await snapshots()).toEqual(before);
    for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT count(*)::int n FROM ${table} WHERE command_id=?`).bind(commandKey).first('n')).toBe(0);
  } finally {
    await db.prepare(`DROP TRIGGER ${trigger} ON outbox_events`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();
  }
  const result=await postAction(db,owner,published.id,input,commandKey);
  expect(await postAction(db,owner,published.id,input,commandKey)).toEqual(result);
  expect(await db.prepare("SELECT version FROM social_posts WHERE id=?").bind(published.id).first('version')).toBe(published.version+1);
  expect(await db.prepare("SELECT count(*)::int n FROM social_post_revisions WHERE post_id=?").bind(published.id).first('n')).toBe(1);
  expect(await db.prepare("SELECT revoked_at FROM attachments WHERE id=?").bind(removed.attachmentId).first('revoked_at')).not.toBeNull();
  expect(await db.prepare("SELECT status FROM upload_intents WHERE id=?").bind(removed.uploadId).first('status')).toBe('expired');
  expect(await db.prepare("SELECT revoked_at FROM attachments WHERE id=?").bind(retained.attachmentId).first('revoked_at')).toBeNull();
  expect(await db.prepare("SELECT status FROM upload_intents WHERE id=?").bind(retained.uploadId).first('status')).toBe('ready');
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT count(*)::int n FROM ${table} WHERE command_id=?`).bind(commandKey).first('n')).toBe(1);
});

async function socialPackage(packageId='work-social',state:'active'|'draining'='active',endsAt:number|null=null){
 const {packageGrantStore,readCompanyPackageGrants}=await import('../../../src/server/modules/commercial/public/package-grants'),prior=(await readCompanyPackageGrants(db,company)).find(row=>row.packageId===packageId);return packageGrantStore(db,{authorize:async()=>({userId:'isolated-social-provider'}),obligations:async()=>{throw Error('Inactive requires real owner sources');}}).apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId:company,packageId,packageVersion:1,state,startsAt:0,endsAt,version:(prior?.version??0)+1,reason:'Actual native Social source fixture',evidence:'test:social-owner-boundary'}});
}
async function socialInactive(){
 const {packageGrantStore,readCompanyPackageGrants,packageGrantRevision}=await import('../../../src/server/modules/commercial/public/package-grants'),{ownerPackageObligations,withOwnerPackageObligations}=await import('../../../src/server/modules/commercial/public/owner-package-obligations'),grants=await readCompanyPackageGrants(db,company),prior=grants.find(row=>row.packageId==='work-social')!,input={companyId:company,expectedRevision:packageGrantRevision(company,grants),idempotencyKey:key(),changes:[{expectedVersion:prior.version,grant:{...prior,state:'inactive' as const,version:prior.version+1,reason:'Actual Social source duties settled',evidence:'test:social-normal-inactive'}}]},store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-social-provider'}),obligations:(database,companyId,packageId)=>ownerPackageObligations(db,database,companyId,packageId)}),review=(apply=false)=>withOwnerPackageObligations(db,{companyId:company,packageId:'work-social',packageVersion:1},database=>apply?store.batch.applyIn(database,input,async()=>{}):store.batch.previewIn(database,input)),preview=await review();expect(await review(true)).toEqual(preview);expect(await review(true)).toEqual(preview);
}
it('purchasedSocial preserves native rich and legacy ownership across no purchase active draining exact retained history and current account revocation',async()=>{
 const native=await import('../../../src/server/modules/collaboration/public/social');vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');const input={body:'Real paid Social update',audience:{kind:'company' as const},expectedVersion:1},legacyInput=native.createPostSchema.parse({body:'Original native workplace update',audience:'holding'}),storyInput=native.createStorySchema.parse({title:'Original native story',body:'Current workplace story'});await expect(savePostDraft(db,owner,null,input,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await expect(native.createPost('h001',legacyInput,key(),owner)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await expect(native.createStory('h001',storyInput,key(),owner)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await socialPackage('work');await expect(savePostDraft(db,owner,null,input,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await socialPackage();const draft=await savePostDraft(db,owner,null,input,key()),publishKey=key(),published=await publishPost(db,owner,draft.id,draft.version,publishKey),legacyKey=key(),legacy=await native.createPost('h001',legacyInput,legacyKey,owner),story=await native.createStory('h001',storyInput,key(),owner);expect(owner.modules).toEqual([]);expect((await postView(db,worker,published.id)).body).toBe(input.body);expect((await native.listFeed('h001',worker)).map(row=>row.id)).toContain(legacy.id);const commentKey=key();await postAction(db,worker,published.id,{action:'comment',body:'Original existing workflow response'},commentKey);
 await socialPackage('work-social','draining');await expect(savePostDraft(db,owner,null,input,key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});await expect(native.createStory('h001',storyInput,key(),owner)).rejects.toMatchObject({code:'PACKAGE_DRAINING'});await expect(postAction(db,owner,published.id,{action:'edit',body:'Unlicensed amended content',expectedVersion:1},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});await postAction(db,worker,published.id,{action:'comment',body:'Settle the existing conversation'},key());await native.createComment('h001',String(legacy.id),native.createCommentSchema.parse({body:'Retained original native response'}),key(),worker);await native.archiveStory('h001',String(story.id),1,key(),owner);await socialInactive();expect((await postView(db,worker,published.id)).body).toBe(input.body);expect(await publishPost(db,owner,draft.id,draft.version,publishKey)).toEqual(published);expect(await publishPost(db,owner,draft.id,draft.version,key())).toEqual(published);expect(await native.createPost('h001',legacyInput,legacyKey,owner)).toEqual(legacy);await expect(postAction(db,worker,published.id,{action:'comment',body:'Inactive new response'},key())).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});await expect(native.createPost('h001',legacyInput,legacyKey,worker)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});expect(await db.prepare('SELECT count(*) n FROM command_receipts WHERE command_id=?').bind(legacyKey).first('n')).toBe(1);
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),worker.assignmentId).run();expect(await homePosts(db,worker)).toEqual([]);await expect(postView(db,worker,published.id)).rejects.toMatchObject({code:'POST_NOT_FOUND'});await expect(postAction(db,worker,published.id,{action:'comment',body:'Original existing workflow response'},commentKey)).rejects.toMatchObject({code:'WORK_SOCIAL_AUTHORITY_CHANGED'});
});
it('purchasedSocial keeps actual Project-only handover source authority before feed limits while ordinary and missing foreign sources cannot license general Social',async()=>{
 const {saveProject,projectSchema}=await import('../../../src/server/modules/projects/public/staff-projects'),{workTaskSocialPurchaseSource}=await import('../../../src/server/modules/collaboration/public/work-task-purchase-source');for(let index=0;index<35;index++)await post();await socialPackage('projects');vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const project=await saveProject(db,owner,null,projectSchema.parse({title:'Actual included Project Social handover',memberUserIds:[worker.userId]}),key()),draft=await savePostDraft(db,owner,null,{body:'Actual private Project handover',audience:{kind:'company'},sources:[{kind:'staff_project',id:project.id}],purpose:'handover',expectedVersion:1},key()),published=await publishPost(db,owner,draft.id,draft.version,key());await db.prepare('UPDATE social_posts SET created_at=created_at+10000 WHERE company_id=? AND id<>?').bind(company,published.id).run();expect((await homePosts(db,owner,{limit:20})).map(row=>row.id)).toEqual([published.id]);expect((await homePosts(db,worker)).map(row=>row.id)).toEqual([published.id]);expect(await homePosts(db,reviewer)).toEqual([]);expect((await workTaskSocialPurchaseSource(db,company,'no-task',published.id)).workSocial).toBe(false);await postAction(db,worker,published.id,{action:'comment',body:'Actual included Project response'},key());await expect(savePostDraft(db,owner,null,{body:'Unlicensed freeform Social',audience:{kind:'project',id:project.id},expectedVersion:1},key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 const task=await saveTask(db,owner,null,taskSchema.parse({title:'Actual included Project Task',projectId:project.id,assigneeUserId:worker.userId}),key()),taskDraft=await savePostDraft(db,owner,null,{body:'Actual Project Task source handover',audience:{kind:'company'},sources:[{kind:'staff_task',id:String(task.id)}],purpose:'handover',expectedVersion:1},key()),taskPost=await publishPost(db,owner,taskDraft.id,taskDraft.version,key());expect((await postView(db,worker,taskPost.id)).body).toBe('Actual Project Task source handover');expect((await workTaskSocialPurchaseSource(db,company,'no-task',taskPost.id)).workSocial).toBe(false);await saveProject(db,owner,project.id,projectSchema.parse({title:'Actual included Project Social handover',memberUserIds:[],expectedVersion:1}),key());await expect(postView(db,worker,published.id)).rejects.toMatchObject({code:'POST_NOT_FOUND'});expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=?').bind(company).all()).results).toEqual([{package_id:'projects'}]);
 await db.prepare("INSERT INTO social_post_sources(post_id,ordinal,source_kind,source_id) VALUES(?,1,'pms_record',?)").bind(published.id,`missing-pms-${key()}`).run();expect((await workTaskSocialPurchaseSource(db,company,'no-task',published.id)).workSocial).toBe(true);expect(await homePosts(db,owner)).not.toContainEqual(expect.objectContaining({id:published.id}));
});
it('purchasedSocial repeats complete rich and legacy source and current IAM after deferred effects and keeps original receipts audit and content atomic before exact retry',async()=>{
 await socialPackage('work');await socialPackage();vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');const native=await import('../../../src/server/modules/collaboration/public/social'),legacyInput=native.createPostSchema.parse({body:'Original native final source',audience:'holding'});
 for(const kind of ['rich-source','rich-authority','legacy-source'] as const){const p=await post(),command=key(),fn=`social_final_${key().replaceAll('-','')}`,input={action:'comment' as const,body:'Reviewed same original Social command'},tables=['social_posts','social_post_drafts','social_comments','social_post_sources','social_post_revisions','assignments','command_receipts','audit_events','outbox_events','staff_notifications'],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r ORDER BY image`).all()).results)),before=await snapshot(),effect=kind==='rich-authority'?`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${owner.assignmentId}'`:kind==='legacy-source'?"UPDATE social_posts SET body='Unreviewed deferred legacy content' WHERE last_command_id=NEW.command_id":`UPDATE social_posts SET body='Unreviewed deferred rich content' WHERE id='${p.id}'`;
  await db.exec(`CREATE SEQUENCE ${fn}_fired;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_fired');${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fn}()`);const run=()=>kind==='legacy-source'?native.createPost('h001',legacyInput,command,owner):postAction(db,owner,p.id,input,command);
  try{await expect(run()).rejects.toMatchObject({code:kind==='rich-authority'?'WORK_SOCIAL_AUTHORITY_CHANGED':'WORK_SOCIAL_SOURCE_CHANGED'});expect(await snapshot()).toEqual(before);expect(await db.prepare(`SELECT is_called FROM ${fn}_fired`).first('is_called')).toBe(true);}finally{await db.exec(`DROP TRIGGER ${fn} ON command_receipts;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_fired`);}const result=await run();expect(await run()).toEqual(result);expect(await db.prepare('SELECT count(*) n FROM command_receipts WHERE command_id=?').bind(command).first('n')).toBe(1);
 }
});
it('purchasedSocial holds native writer sources across two Companies with no hidden deadlock retry and keeps each original actor and Company',async()=>{
 await socialPackage('work');await socialPackage();const {expenseFixture}=await import('../../support/helpers/expense-fixture'),other=await expenseFixture(),{packageGrantStore}=await import('../../../src/server/modules/commercial/public/package-grants');for(const packageId of ['work','work-social'])await packageGrantStore(db,{authorize:async()=>({userId:'isolated-second-social-provider'}),obligations:async()=>{throw Error('No withdrawal');}}).apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:other.company,packageId,packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Actual separate Company Social source',evidence:'test:social-two-company'}});vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');const fn=`social_parallel_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.1);RETURN NEW;END $$;CREATE TRIGGER ${fn} BEFORE INSERT ON social_post_drafts FOR EACH ROW WHEN(NEW.company_id IN('${company}','${other.company}')) EXECUTE FUNCTION ${fn}()`);
 try{const {measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics'),observed=await measureReads(()=>Promise.all([savePostDraft(db,owner,null,{body:'Actual first Company source',audience:{kind:'company'},expectedVersion:1},key()),savePostDraft(db,other.owner,null,{body:'Actual second Company source',audience:{kind:'company'},expectedVersion:1},key())]));expect(observed.metrics.deadlockFailures??0).toBe(0);for(const [index,actor] of [owner,other.owner].entries())expect(await db.prepare('SELECT company_id,author_id,version FROM social_post_drafts WHERE id=?').bind(observed.result[index].id).first()).toEqual({company_id:actor.companyId,author_id:actor.userId,version:1});}finally{await db.exec(`DROP TRIGGER ${fn} ON social_post_drafts;DROP FUNCTION ${fn}()`);}
});
it('purchasedSocial repeats the actual new-work clock after deferred effects and rolls native content journals and purchase facts back before normal renewal retry',async()=>{
 await socialPackage('work');await socialPackage('work-social','active',Date.now()+5000);vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');const command=key(),id=`post-draft-${command}`,fn=`social_expiry_${key().replaceAll('-','')}`,input={body:'Actual deferred Social purchase clock',audience:{kind:'company' as const},expectedVersion:1},tables=['company_package_grants','company_package_grant_events','social_post_drafts','command_receipts','audit_events','outbox_events'],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r ORDER BY image`).all()).results)),before=await snapshot();await db.exec(`CREATE SEQUENCE ${fn}_fired;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_fired');PERFORM pg_sleep(5.2);RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fn}()`);
 try{await expect(savePostDraft(db,owner,null,input,command)).rejects.toMatchObject({code:'PACKAGE_CAPABILITY_CHANGED'});expect(await snapshot()).toEqual(before);expect(await db.prepare(`SELECT is_called FROM ${fn}_fired`).first('is_called')).toBe(true);}finally{await db.exec(`DROP TRIGGER ${fn} ON command_receipts;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_fired`);}await socialPackage();const result=await savePostDraft(db,owner,null,input,command);expect(result.id).toBe(id);expect(await savePostDraft(db,owner,null,input,command)).toEqual(result);expect(await db.prepare('SELECT count(*) n FROM command_receipts WHERE command_id=?').bind(command).first('n')).toBe(1);
});

const purchasedAiOutput=(postId:string)=>({result:{summary:'Actual current source summary',title:null,description:null,assigneeUserId:null,reviewerUserId:null,dueAt:null,missing:[],citations:[postId]},inputTokens:3,outputTokens:2});
it('purchasedSocialAI reserves active work outside provider IO then settles draining and replays retained exact private output while new reservations stay closed',async()=>{
 const p=await post(),provider=vi.fn(async()=>purchasedAiOutput(p.id));vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');
 await expect(createAiDraft(db,owner,p.id,{kind:'summary',commentIds:[]},{},provider,key())).rejects.toMatchObject({code:'POST_NOT_FOUND'});expect(provider).not.toHaveBeenCalled();
 await socialPackage('work');await socialPackage();const command=key(),result=await createAiDraft(db,owner,p.id,{kind:'summary',commentIds:[]},{},async()=>{expect(await db.prepare("SELECT state FROM social_ai_usage WHERE company_id=?").bind(company).first('state')).toBe('reserved');await socialPackage('work-social','draining');return purchasedAiOutput(p.id);},command);
 expect(await db.prepare('SELECT state,lease_until,input_tokens,output_tokens FROM social_ai_usage WHERE id=?').bind(result.id).first()).toEqual({state:'succeeded',lease_until:0,input_tokens:3,output_tokens:2});expect(await createAiDraft(db,owner,p.id,{kind:'summary',commentIds:[]},{},provider,command)).toEqual(result);expect(provider).not.toHaveBeenCalled();
 await expect(createAiDraft(db,owner,p.id,{kind:'summary',commentIds:[]},{},provider,key())).rejects.toMatchObject({code:'AI_LIMIT'});expect(provider).not.toHaveBeenCalled();
});
it('purchasedSocialAI closes original billed reservations after current IAM revocation without returning output or creating work',async()=>{
 await socialPackage('work');await socialPackage();vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');const p=await post(),provider=vi.fn(async()=>{await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(owner.assignmentId).run();return purchasedAiOutput(p.id);});
 await expect(createAiDraft(db,owner,p.id,{kind:'summary',commentIds:[]},{},provider,key())).rejects.toMatchObject({code:'AI_UNAVAILABLE'});expect(provider).toHaveBeenCalledOnce();expect(await db.prepare('SELECT d.state,d.result_json,u.state usage_state,u.lease_until FROM social_ai_drafts d JOIN social_ai_usage u ON u.id=d.id WHERE d.post_id=?').bind(p.id).first()).toEqual({state:'failed',result_json:null,usage_state:'failed',lease_until:0});expect(await db.prepare('SELECT count(*) n FROM social_task_origins WHERE post_id=?').bind(p.id).first('n')).toBe(0);
});
it('purchasedSocialAI rolls back late deferred ready output source and IAM changes then durably closes only the failed reservation',async()=>{
 await socialPackage('work');await socialPackage();vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');const p=await post();
 for(const kind of ['authority','result'] as const){const fn=`social_ai_final_${key().replaceAll('-','')}`,effect=kind==='authority'?`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${owner.assignmentId}'`:`UPDATE social_ai_drafts SET result_json='{"summary":"Unreviewed output"}' WHERE id=NEW.id`;
 await db.exec(`CREATE SEQUENCE ${fn}_fired;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_fired');${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER UPDATE ON social_ai_drafts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.state='ready' AND OLD.state='pending') EXECUTE FUNCTION ${fn}()`);
 try{await expect(createAiDraft(db,owner,p.id,{kind:'summary',commentIds:[]},{},async()=>purchasedAiOutput(p.id),key())).rejects.toMatchObject({code:'AI_UNAVAILABLE'});expect(await db.prepare(`SELECT is_called FROM ${fn}_fired`).first('is_called')).toBe(true);expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(owner.assignmentId).first()).toEqual({revoked_at:null,version:1});expect((await db.prepare('SELECT d.state,d.result_json,u.state usage_state,u.output_tokens FROM social_ai_drafts d JOIN social_ai_usage u ON u.id=d.id WHERE d.post_id=?').bind(p.id).all()).results).toEqual(Array.from({length:kind==='authority'?1:2},()=>({state:'failed',result_json:null,usage_state:'failed',output_tokens:0})));}
 finally{await db.exec(`DROP TRIGGER ${fn} ON social_ai_drafts;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_fired`);}}
});
it('purchasedSocialAI claims only actual captured queued authority before its limit and retains pending raw duties until original completion',async()=>{
 await socialPackage('work');await socialPackage();vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');const p=await assistantPost(),original=(await db.prepare('SELECT * FROM social_agent_replies WHERE post_id=?').bind(p.id).first<{id:string;actor_json:string;created_at:number}>())!;
 const captured=JSON.parse(original.actor_json),legacy=Array.from({length:105},(_,i)=>({id:`legacy-queued-${key()}`,comment:`legacy-question-${key()}`,at:original.created_at-1000+i,actor:i%3===0?'not-json':i%3===1?'{}':JSON.stringify({...captured,iam:{...captured.iam,queuedAssignmentVersion:'malformed'}})}));
 await db.batch(legacy.flatMap(row=>[db.prepare("INSERT INTO social_comments(id,post_id,author_id,author_name,body,created_at,updated_at) VALUES(?,?,?,'Owner','Actual populated older question',?,?)").bind(row.comment,p.id,owner.userId,row.at,row.at),db.prepare("INSERT INTO social_agent_replies(id,post_id,owner_type,owner_id,company_id,actor_id,actor_json,state,created_at,updated_at) VALUES(?,?,'comment',?,?,?,?, 'pending',?,?)").bind(row.id,p.id,row.comment,company,owner.userId,row.actor,row.at,row.at)]));
 const {ownerPackageObligations,withOwnerPackageObligations}=await import('../../../src/server/modules/commercial/public/owner-package-obligations'),observe=()=>withOwnerPackageObligations(db,{companyId:company,packageId:'work-social',packageVersion:1},database=>ownerPackageObligations(db,database,company,'work-social')),before=await observe(),provider=vi.fn(async()=>({result:{body:'Actual original captured assistant response'},inputTokens:2,outputTokens:1}));
 await socialPackage('work-social','draining');expect(await drainSocialAgent(db,replyEnv,undefined,provider)).toBe(true);expect(provider).toHaveBeenCalledOnce();expect(await db.prepare('SELECT state,attempt FROM social_agent_replies WHERE id=?').bind(original.id).first()).toEqual({state:'completed',attempt:1});expect(await db.prepare("SELECT count(*) n FROM social_agent_replies WHERE post_id=? AND state='failed' AND error_code='ACCESS_CHANGED' AND attempt=0").bind(p.id).first('n')).toBe(105);expect(await drainSocialAgent(db,replyEnv,undefined,provider)).toBe(false);expect(provider).toHaveBeenCalledOnce();
 const after=await observe();expect(before).toBeGreaterThanOrEqual(106);expect(after).toBe(before-105); //105 unclaimable questions terminate without spend; the ready private draft replaces the completed valid reply duty.
 expect((await db.prepare('SELECT actor_json FROM social_agent_replies WHERE id=?').bind(legacy[0].id).first('actor_json'))).toBe(legacy[0].actor);expect(await db.prepare('SELECT count(*) n FROM social_ai_usage WHERE company_id=?').bind(company).first('n')).toBe(1);await retryAgentReply(db,owner,legacy[0].id,key());expect(await drainSocialAgent(db,replyEnv,p.id,provider)).toBe(true);expect(provider).toHaveBeenCalledTimes(2);expect(await db.prepare('SELECT state,attempt FROM social_agent_replies WHERE id=?').bind(legacy[0].id).first()).toEqual({state:'completed',attempt:1});expect(await db.prepare('SELECT count(*) n FROM social_ai_usage WHERE company_id=?').bind(company).first('n')).toBe(2);
});
it('purchasedSocialAI preserves actual Projects-only foundation without Work or People grants and repeats queued native source after deferred claim effects',async()=>{
 const {saveProject,projectSchema}=await import('../../../src/server/modules/projects/public/staff-projects');await socialPackage('projects');vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const project=await saveProject(db,owner,null,projectSchema.parse({title:'Actual Project AI source',memberUserIds:[worker.userId]}),key()),draft=await savePostDraft(db,owner,null,{body:'@ERPX assistant Help #ทีมงาน',mentions:[assistantMention],audience:{kind:'company'},sources:[{kind:'staff_project',id:project.id}],purpose:'handover',expectedVersion:1},key()),p=await publishPost(db,owner,draft.id,draft.version,key());
 const direct=await createAiDraft(db,worker,p.id,{kind:'summary',commentIds:[]},{},async()=>purchasedAiOutput(p.id),key());expect(direct.result.summary).toBe('Actual current source summary');const original=(await db.prepare('SELECT id FROM social_agent_replies WHERE post_id=?').bind(p.id).first<{id:string}>())!,fn=`social_ai_claim_${key().replaceAll('-','')}`,provider=vi.fn(async()=>({result:{body:'Only original current Project source'},inputTokens:1,outputTokens:1}));
 await db.exec(`CREATE SEQUENCE ${fn}_fired;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_fired');UPDATE staff_projects SET title='Unreviewed deferred Project source' WHERE id='${project.id}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER UPDATE ON social_agent_replies DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.id='${original.id}' AND NEW.state='running' AND OLD.state='pending') EXECUTE FUNCTION ${fn}()`);
 try{await expect(drainSocialAgent(db,replyEnv,p.id,provider)).rejects.toMatchObject({code:'WORK_SOCIAL_SOURCE_CHANGED'});expect(provider).not.toHaveBeenCalled();expect(await db.prepare('SELECT state,attempt FROM social_agent_replies WHERE id=?').bind(original.id).first()).toEqual({state:'pending',attempt:0});expect(await db.prepare('SELECT title FROM staff_projects WHERE id=?').bind(project.id).first('title')).toBe('Actual Project AI source');expect(await db.prepare(`SELECT is_called FROM ${fn}_fired`).first('is_called')).toBe(true);}
 finally{await db.exec(`DROP TRIGGER ${fn} ON social_agent_replies;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_fired`);}
 expect(await drainSocialAgent(db,replyEnv,p.id,provider)).toBe(true);expect(provider).toHaveBeenCalledOnce();expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=?').bind(company).all()).results).toEqual([{package_id:'projects'}]);
});
it('purchasedSocialAI keeps later queued output atomic when current authority changes during provider work and closes the native failed lease',async()=>{
 await socialPackage('work');await socialPackage();vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');const p=await assistantPost(),provider=vi.fn(async()=>{await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(owner.assignmentId).run();return {result:{body:'This must remain private and unpublished'},inputTokens:1,outputTokens:1};});
 expect(await drainSocialAgent(db,replyEnv,p.id,provider)).toBe(true);expect(provider).toHaveBeenCalledOnce();expect(await db.prepare('SELECT state,error_code,lease_until FROM social_agent_replies WHERE post_id=?').bind(p.id).first()).toEqual({state:'failed',error_code:'ACCESS_CHANGED',lease_until:0});expect(await db.prepare("SELECT count(*) n FROM social_comments WHERE post_id=? AND author_kind='agent'").bind(p.id).first('n')).toBe(0);expect(await db.prepare('SELECT d.state,d.result_json,u.state usage_state,u.lease_until FROM social_ai_drafts d JOIN social_ai_usage u ON u.id=d.id WHERE d.post_id=?').bind(p.id).first()).toEqual({state:'failed',result_json:null,usage_state:'failed',lease_until:0});
});
it('purchasedSocialAI releases reservation transactions before two Companies call the provider and completes both without hidden deadlock retries',async()=>{
 const p=await post(),{expenseFixture}=await import('../../support/helpers/expense-fixture'),other=await expenseFixture(),draft=await savePostDraft(db,other.owner,null,{body:'Actual second Company AI source',audience:{kind:'company'},expectedVersion:1},key()),second=await publishPost(db,other.owner,draft.id,draft.version,key()),{packageGrantStore}=await import('../../../src/server/modules/commercial/public/package-grants');await socialPackage('work');await socialPackage();for(const packageId of ['work','work-social'])await packageGrantStore(db,{authorize:async()=>({userId:'isolated-second-ai-provider'}),obligations:async()=>{throw Error('No withdrawal');}}).apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:other.company,packageId,packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Actual separate Company AI reservation',evidence:'test:social-ai-two-company'}});vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');
 let arrived=0,release=()=>{};const bothProviders=new Promise<void>(resolve=>{release=resolve;}),provider=(postId:string)=>async()=>{arrived++;if(arrived===2)release();await bothProviders;return purchasedAiOutput(postId);},{measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics'),observed=await measureReads(()=>Promise.all([createAiDraft(db,owner,p.id,{kind:'summary',commentIds:[]},{},provider(p.id),key()),createAiDraft(db,other.owner,second.id,{kind:'summary',commentIds:[]},{},provider(second.id),key())]));
 expect(arrived).toBe(2);expect(observed.metrics.deadlockFailures??0).toBe(0);for(const[index,actor]of[owner,other.owner].entries())expect(await db.prepare('SELECT company_id,actor_id,state,lease_until FROM social_ai_usage WHERE id=?').bind(observed.result[index].id).first()).toEqual({company_id:actor.companyId,actor_id:actor.userId,state:'succeeded',lease_until:0});
});
async function nativeSocialImageAssistant(actor=owner){
 const providers=await import('../../../src/server/modules/agent/public/ai-providers'),imageEnv={...replyEnv,AI_CREDENTIAL_KEY_ID:'v1',AI_CREDENTIAL_KEYS:JSON.stringify({v1:btoa(String.fromCharCode(...new Uint8Array(32).fill(23)))})},connection=await providers.saveAiConnection(db,actor,imageEnv,null,providers.aiConnectionSchema.parse({name:'Actual private Social image model',baseUrl:'https://opencode.ai/zen/v1',apiKey:'synthetic-native-image-key',textModel:'gpt-5.6-luna',imageModel:'claude-sonnet-4-6'}),key());
 await providers.testAiConnection(db,actor,imageEnv,connection.id,1,key(),async(_endpoint,messages)=>({result:Array.isArray(messages[1].content)?{color:'red'}:{ok:true},inputTokens:1,outputTokens:1}));await providers.activateAiConnection(db,actor,connection.id,2,key());await providers.selectAiProviders(db,actor,providers.aiSelectionSchema.parse({expectedVersion:0,mode:'custom',textConnectionId:connection.id}),key());
 const {default:sharp}=await import('sharp'),bytes=Uint8Array.from(await sharp({create:{width:1,height:1,channels:4,background:{r:255,g:0,b:0,alpha:1}}}).png().toBuffer()),{sha256}=await import('../../../src/server/modules/runtime/public/crypto'),hash=await sha256(bytes.buffer),slotId=key(),body='@ERPX assistant Help #ทีมงาน',draft=await savePostDraft(db,actor,null,{body,mentions:[assistantMention],content:[{type:'text',text:body},{type:'image',slotId,attachmentId:null,name:'native-private.png'}],audience:{kind:'company'},expectedVersion:1},key()),image=await readySocialImage(draft.id,'native-private.png',{checksum:hash,bytes:bytes.byteLength,actorId:actor.userId}),objectKey=(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(image.attachmentId).first<string>('object_key'))!;
 await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:'image/png'}});
 const saved=await savePostDraft(db,actor,draft.id,{body,mentions:[assistantMention],content:[{type:'text',text:body},{type:'image',slotId,attachmentId:image.attachmentId,name:'native-private.png'}],audience:{kind:'company'},expectedVersion:draft.version},key()),p=await publishPost(db,actor,draft.id,saved.version,key());return {p,image,imageEnv,objectKey};
}
it('purchasedSocialAI buffers actual private native images once under current source authority before invoking its original provider',async()=>{
 const {p,image,imageEnv,objectKey}=await nativeSocialImageAssistant();await socialPackage('work');await socialPackage();vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');let consumed=0;const files:import('../../../src/server/modules/runtime/public/runtime/types').FileStore={put:(...args)=>env.H001_FILES.put(...args),head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args),get:async(...args)=>{expect(args[0]).toBe(objectKey);const object=await env.H001_FILES.get(...args);return object?{...object,arrayBuffer:async()=>{consumed++;return object.arrayBuffer();}}:null;}},provider=vi.fn(async(_endpoint:unknown,messages:{content:unknown}[])=>{expect(consumed).toBe(1);expect(JSON.stringify(messages[1].content)).toContain('data:image/jpeg;base64,');return {result:{body:'Actual authorized image reply'},inputTokens:2,outputTokens:1};});
 expect(await drainSocialAgent(db,imageEnv,p.id,provider,files,owner.holdingId)).toBe(true);expect(provider).toHaveBeenCalledOnce();expect(consumed).toBe(1);expect(await db.prepare("SELECT count(*) n FROM social_comments WHERE post_id=? AND author_kind='agent'").bind(p.id).first('n')).toBe(1);expect(await db.prepare('SELECT revoked_at FROM attachments WHERE id=?').bind(image.attachmentId).first('revoked_at')).toBeNull();
});
it('purchasedSocialAI withholds buffered private image bytes from provider after deferred Files source or current IAM changes and preserves original failed cleanup',async()=>{
 const {p,image,imageEnv}=await nativeSocialImageAssistant();await socialPackage('work');await socialPackage();vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');const reply=(await db.prepare('SELECT id FROM social_agent_replies WHERE post_id=?').bind(p.id).first<{id:string}>())!;
 for(const kind of ['file','source','authority','provider'] as const){if(kind!=='file')await retryAgentReply(db,owner,reply.id,key());const name=`social_ai_bytes_${key().replaceAll('-','')}`,effect=kind==='file'?`UPDATE attachments SET revoked_at=1 WHERE id='${image.attachmentId}'`:kind==='source'?`UPDATE social_posts SET body='Unreviewed buffered private source' WHERE id='${p.id}'`:kind==='authority'?`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${owner.assignmentId}'`:`UPDATE ai_company_settings SET mode='disabled',version=version+1 WHERE company_id='${company}'`;
 await db.exec(`CREATE SEQUENCE ${name}_fired;CREATE FUNCTION ${name}_schedule() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id LIKE 'work-social:%' AND current_setting('erpx.social_ai_image_fault',true) IS DISTINCT FROM '1' AND EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND relation='attachments'::regclass AND mode='ShareRowExclusiveLock' AND granted) THEN PERFORM set_config('erpx.social_ai_image_fault','1',true);UPDATE social_ai_drafts SET actor_json=actor_json WHERE post_id='${p.id}' AND state='pending';END IF;RETURN NEW;END $$;CREATE TRIGGER ${name}_schedule INSTEAD OF INSERT ON command_assertions FOR EACH ROW EXECUTE FUNCTION ${name}_schedule();CREATE FUNCTION ${name}_change() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF current_setting('erpx.social_ai_image_fault',true)='1' THEN PERFORM nextval('${name}_fired');${effect};END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${name}_change AFTER UPDATE ON social_ai_drafts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.post_id='${p.id}' AND NEW.state='pending' AND OLD.state='pending') EXECUTE FUNCTION ${name}_change()`);
 let consumed=0;const files:import('../../../src/server/modules/runtime/public/runtime/types').FileStore={put:(...args)=>env.H001_FILES.put(...args),head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args),get:async(...args)=>{const object=await env.H001_FILES.get(...args);return object?{...object,arrayBuffer:async()=>{consumed++;return object.arrayBuffer();}}:null;}},provider=vi.fn(async()=>({result:{body:'Never expose buffered bytes'},inputTokens:1,outputTokens:1}));
 try{expect(await drainSocialAgent(db,imageEnv,p.id,provider,files,owner.holdingId)).toBe(true);expect(consumed).toBe(1);expect(provider).not.toHaveBeenCalled();expect(await db.prepare(`SELECT is_called FROM ${name}_fired`).first('is_called')).toBe(true);expect(await db.prepare('SELECT revoked_at FROM attachments WHERE id=?').bind(image.attachmentId).first('revoked_at')).toBeNull();expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(owner.assignmentId).first()).toEqual({revoked_at:null,version:1});expect(await db.prepare('SELECT body FROM social_posts WHERE id=?').bind(p.id).first('body')).toBe('@ERPX assistant Help #ทีมงาน');expect(await db.prepare('SELECT version,mode FROM ai_company_settings WHERE company_id=?').bind(company).first()).toEqual({version:1,mode:'custom'});expect(await db.prepare('SELECT state,lease_until FROM social_agent_replies WHERE id=?').bind(reply.id).first()).toEqual({state:'failed',lease_until:0});expect(await db.prepare("SELECT count(*) n FROM social_comments WHERE post_id=? AND author_kind='agent'").bind(p.id).first('n')).toBe(0);}
 finally{await db.exec(`DROP TRIGGER ${name}_schedule ON command_assertions;DROP TRIGGER ${name}_change ON social_ai_drafts;DROP FUNCTION ${name}_schedule();DROP FUNCTION ${name}_change();DROP SEQUENCE ${name}_fired`);}}
});
it('purchasedSocialAI completes two Companies actual private image phases without holding provider IO or hiding metadata lease deadlocks',async()=>{
 const {expenseFixture}=await import('../../support/helpers/expense-fixture'),other=await expenseFixture(),first=await nativeSocialImageAssistant(),second=await nativeSocialImageAssistant(other.owner),{packageGrantStore}=await import('../../../src/server/modules/commercial/public/package-grants');await socialPackage('work');await socialPackage();for(const packageId of ['work','work-social'])await packageGrantStore(db,{authorize:async()=>({userId:'isolated-second-image-provider'}),obligations:async()=>{throw Error('No withdrawal');}}).apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:other.company,packageId,packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Actual separate Company private image source',evidence:'test:social-image-two-company'}});vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');
 let arrived=0,consumed=0,release=()=>{};const bothProviders=new Promise<void>(resolve=>{release=resolve;}),files:import('../../../src/server/modules/runtime/public/runtime/types').FileStore={put:(...args)=>env.H001_FILES.put(...args),head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args),get:async(...args)=>{expect([first.objectKey,second.objectKey]).toContain(args[0]);const object=await env.H001_FILES.get(...args);return object?{...object,arrayBuffer:async()=>{consumed++;return object.arrayBuffer();}}:null;}},provider=async()=>{arrived++;if(arrived===2)release();await bothProviders;return {result:{body:'Actual separate Company private image answer'},inputTokens:1,outputTokens:1};},{measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics'),observed=await measureReads(()=>Promise.all([drainSocialAgent(db,first.imageEnv,first.p.id,provider,files,owner.holdingId),drainSocialAgent(db,second.imageEnv,second.p.id,provider,files,other.owner.holdingId)]));
 expect(observed.result).toEqual([true,true]);expect(arrived).toBe(2);expect(consumed).toBe(2);expect(observed.metrics.deadlockFailures??0).toBe(0);for(const item of [first,second])expect(await db.prepare("SELECT count(*) n FROM social_comments WHERE post_id=? AND author_kind='agent'").bind(item.p.id).first('n')).toBe(1);
});
it('purchasedSocialAI reconciles the original timed out reservation idempotently after normal native cleanup while provider IO was outstanding',async()=>{
 await socialPackage('work');await socialPackage();vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');const p=await post(),{expireBetaAiReservations}=await import('../../../src/server/modules/collaboration/public/beta-resource-cleanup'),provider=vi.fn(async()=>{await expireBetaAiReservations(db,Date.now()+1,Date.now()+180001);return purchasedAiOutput(p.id);});
 await expect(createAiDraft(db,owner,p.id,{kind:'summary',commentIds:[]},{},provider,key())).rejects.toMatchObject({code:'AI_UNAVAILABLE'});expect(provider).toHaveBeenCalledOnce();expect(await db.prepare('SELECT d.state,d.result_json,u.state usage_state,u.lease_until FROM social_ai_drafts d JOIN social_ai_usage u ON u.id=d.id WHERE d.post_id=?').bind(p.id).first()).toEqual({state:'failed',result_json:null,usage_state:'failed',lease_until:0});
});
it('purchasedSocialAI closes only the original expired reservation after an explicit native retry completes a newer lease and never overwrites that reply',async()=>{
 await socialPackage('work');await socialPackage();vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');const p=await assistantPost(),reply=(await db.prepare('SELECT id FROM social_agent_replies WHERE post_id=?').bind(p.id).first<{id:string}>())!,newProvider=vi.fn(async()=>({result:{body:'Actual later authorized native reply'},inputTokens:2,outputTokens:1})),oldProvider=vi.fn(async()=>{await db.prepare('UPDATE social_agent_replies SET lease_until=1 WHERE id=?').bind(reply.id).run();expect(await drainSocialAgent(db,replyEnv,p.id,vi.fn())).toBe(false);await retryAgentReply(db,owner,reply.id,key());expect(await drainSocialAgent(db,replyEnv,p.id,newProvider)).toBe(true);return {result:{body:'Never overwrite the newer native response'},inputTokens:1,outputTokens:1};});
 expect(await drainSocialAgent(db,replyEnv,p.id,oldProvider)).toBe(true);expect(oldProvider).toHaveBeenCalledOnce();expect(newProvider).toHaveBeenCalledOnce();expect(await db.prepare('SELECT state,attempt,lease_until FROM social_agent_replies WHERE id=?').bind(reply.id).first()).toEqual({state:'completed',attempt:2,lease_until:0});expect((await db.prepare("SELECT body FROM social_comments WHERE post_id=? AND author_kind='agent'").bind(p.id).all()).results).toEqual([{body:'Actual later authorized native reply'}]);expect((await db.prepare('SELECT id,state,lease_until FROM social_ai_usage WHERE company_id=? ORDER BY id').bind(company).all()).results).toEqual([{id:`${reply.id}:1`,state:'failed',lease_until:0},{id:`${reply.id}:2`,state:'succeeded',lease_until:0}]);
});
it('purchasedSocialAI fails only truly unclaimable pending authority without provider spend and retains the original explicit recovery workflow',async()=>{
 await socialPackage('work');await socialPackage();vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');const p=await assistantPost(),original=(await db.prepare('SELECT id,actor_json FROM social_agent_replies WHERE post_id=?').bind(p.id).first<{id:string;actor_json:string}>())!,provider=vi.fn(async()=>({result:{body:'Actual explicitly recovered authority reply'},inputTokens:1,outputTokens:1})),{ownerPackageObligations,withOwnerPackageObligations}=await import('../../../src/server/modules/commercial/public/owner-package-obligations'),observe=()=>withOwnerPackageObligations(db,{companyId:company,packageId:'work-social',packageVersion:1},database=>ownerPackageObligations(db,database,company,'work-social')),before=await observe();
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(owner.assignmentId).run();expect(await drainSocialAgent(db,replyEnv,p.id,provider)).toBe(false);expect(provider).not.toHaveBeenCalled();expect(await db.prepare('SELECT state,error_code,attempt,lease_until,actor_json FROM social_agent_replies WHERE id=?').bind(original.id).first()).toEqual({state:'failed',error_code:'ACCESS_CHANGED',attempt:0,lease_until:0,actor_json:original.actor_json});expect(await db.prepare('SELECT count(*) n FROM social_ai_usage WHERE company_id=?').bind(company).first('n')).toBe(0);expect(await observe()).toBe(before-1);await expect(retryAgentReply(db,owner,original.id,key())).rejects.toMatchObject({code:'WORK_SOCIAL_AUTHORITY_CHANGED'});
 await db.prepare('UPDATE assignments SET revoked_at=NULL,version=version+1 WHERE id=?').bind(owner.assignmentId).run();await freshIam(owner);await retryAgentReply(db,owner,original.id,key());const captured=JSON.parse((await db.prepare('SELECT actor_json FROM social_agent_replies WHERE id=?').bind(original.id).first<string>('actor_json'))!);expect(captured.iam.queuedAssignmentVersion).toBe(3);expect(captured.authority.assignmentVersion).toBe(3);expect(await drainSocialAgent(db,replyEnv,p.id,provider)).toBe(true);expect(provider).toHaveBeenCalledOnce();expect(await db.prepare('SELECT state,attempt FROM social_agent_replies WHERE id=?').bind(original.id).first()).toEqual({state:'completed',attempt:1});expect(await db.prepare('SELECT count(*) n FROM assignments WHERE id=?').bind(owner.assignmentId).first('n')).toBe(1);
});
it('purchasedSocialAI rolls back no-output queue failure when deferred source or captured authority becomes valid before the final native check',async()=>{
 await socialPackage('work');await socialPackage();vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');const p=await assistantPost(),reply=(await db.prepare('SELECT id,actor_json FROM social_agent_replies WHERE post_id=?').bind(p.id).first<{id:string;actor_json:string}>())!,provider=vi.fn(async()=>({result:{body:'Do not run while original authority is revoked'},inputTokens:1,outputTokens:1}));await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(owner.assignmentId).run();
 for(const kind of ['source','authority'] as const){const name=`social_ai_unclaimable_${key().replaceAll('-','')}`,effect=kind==='source'?`UPDATE social_posts SET body='Unreviewed deferred cleanup source' WHERE id='${p.id}'`:`UPDATE assignments SET revoked_at=NULL,version=1 WHERE id='${owner.assignmentId}'`;
 await db.exec(`CREATE SEQUENCE ${name}_fired;CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${name}_fired');${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${name} AFTER UPDATE ON social_agent_replies DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.id='${reply.id}' AND NEW.state='failed' AND NEW.error_code='ACCESS_CHANGED') EXECUTE FUNCTION ${name}()`);
 try{await expect(drainSocialAgent(db,replyEnv,p.id,provider)).rejects.toMatchObject({code:'WORK_SOCIAL_SOURCE_CHANGED'});expect(provider).not.toHaveBeenCalled();expect(await db.prepare(`SELECT is_called FROM ${name}_fired`).first('is_called')).toBe(true);expect(await db.prepare('SELECT state,attempt,actor_json FROM social_agent_replies WHERE id=?').bind(reply.id).first()).toEqual({state:'pending',attempt:0,actor_json:reply.actor_json});expect(await db.prepare('SELECT body FROM social_posts WHERE id=?').bind(p.id).first('body')).toBe('@ERPX assistant Help #ทีมงาน');expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(owner.assignmentId).first()).toEqual({revoked_at:1,version:2});expect(await db.prepare('SELECT count(*) n FROM social_ai_usage WHERE company_id=?').bind(company).first('n')).toBe(0);}
 finally{await db.exec(`DROP TRIGGER ${name} ON social_agent_replies;DROP FUNCTION ${name}();DROP SEQUENCE ${name}_fired`);}}
 expect(await drainSocialAgent(db,replyEnv,p.id,provider)).toBe(false);expect(provider).not.toHaveBeenCalled();expect(await db.prepare('SELECT state,attempt FROM social_agent_replies WHERE id=?').bind(reply.id).first()).toEqual({state:'failed',attempt:0});
});
it('purchasedSocialAI binds the exact captured context and native question across provider IO and cannot overwrite a rebound instruction while closing old usage',async()=>{
 await socialPackage('work');await socialPackage();vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');const {captureJobAuthority}=await import('../../../src/server/modules/identity_access/public/iam-jobs');
 for(const kind of ['context','question','source-author'] as const){const p=await assistantPost(),commentKey=key();await postAction(db,owner,p.id,{action:'comment',body:'A distinct already published original question'},commentKey);const reply=(await db.prepare('SELECT id,actor_json FROM social_agent_replies WHERE post_id=?').bind(p.id).first<{id:string;actor_json:string}>())!,other=await captureJobAuthority(db,await freshIam({...owner})),otherJson=JSON.stringify({userId:other.userId,assignmentId:other.assignmentId,role:other.role,companyId:other.companyId,branchId:other.branchId,departmentId:other.departmentId,iam:other.iam,authority:other.authority}),provider=vi.fn(async()=>{if(kind==='context')await db.prepare('UPDATE social_agent_replies SET actor_json=? WHERE id=?').bind(otherJson,reply.id).run();else if(kind==='question')await db.prepare("UPDATE social_agent_replies SET owner_type='comment',owner_id=? WHERE id=?").bind(`comment-${commentKey}`,reply.id).run();else await db.prepare('UPDATE social_posts SET author_id=? WHERE id=?').bind(worker.userId,p.id).run();return {result:{body:'Never publish under a rebound instruction'},inputTokens:1,outputTokens:1};});
 expect(await drainSocialAgent(db,replyEnv,p.id,provider)).toBe(true);expect(provider).toHaveBeenCalledOnce();expect(await db.prepare("SELECT count(*) n FROM social_comments WHERE post_id=? AND author_kind='agent'").bind(p.id).first('n')).toBe(0);expect(await db.prepare('SELECT state,lease_until FROM social_ai_usage WHERE id=?').bind(`${reply.id}:1`).first()).toEqual({state:'failed',lease_until:0});expect(await db.prepare('SELECT state,actor_json,owner_type,owner_id,attempt FROM social_agent_replies WHERE id=?').bind(reply.id).first()).toEqual({state:kind==='source-author'?'failed':'running',actor_json:kind==='context'?otherJson:reply.actor_json,owner_type:kind==='question'?'comment':'post',owner_id:kind==='question'?`comment-${commentKey}`:p.id,attempt:1});
 if(kind==='source-author'){expect(await db.prepare('SELECT author_id FROM social_posts WHERE id=?').bind(p.id).first('author_id')).toBe(worker.userId);continue;}
 await db.prepare('UPDATE social_agent_replies SET lease_until=1 WHERE id=?').bind(reply.id).run();const next=vi.fn(async()=>({result:{body:'Actual current explicit native retry after source review'},inputTokens:1,outputTokens:1}));expect(await drainSocialAgent(db,replyEnv,p.id,next)).toBe(false);expect(next).not.toHaveBeenCalled();await retryAgentReply(db,kind==='context'?other:owner,reply.id,key());expect(await drainSocialAgent(db,replyEnv,p.id,next)).toBe(true);expect(next).toHaveBeenCalledOnce();expect(await db.prepare('SELECT state,attempt FROM social_agent_replies WHERE id=?').bind(reply.id).first()).toEqual({state:'completed',attempt:2});}
});

it('purchasedSocialPresentation emits minimal current active draining and retained flags for the actual post and current selected account',async()=>{
 const {socialPresentation}=await import('../../../src/server/modules/collaboration/public/social-home');await socialPackage('work');await socialPackage();vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');const p=await post(),active=await postView(db,worker,p.id),context=await socialPresentation(db,worker);
 expect(active.purchase).toMatchObject({newWork:true,settleExisting:true,history:true,createTask:true});expect(context.purchase).toMatchObject({newWork:true,settleExisting:true,history:true});expect(context.context).toMatch(/^[a-f0-9]{64}$/);expect(Object.keys(active.purchase!).sort()).toEqual(['createTask','history','newWork','revision','settleExisting']);
 await socialPackage('work-social','draining');const draining=await postView(db,worker,p.id);expect(draining.purchase).toMatchObject({newWork:false,settleExisting:true,history:true,createTask:false});expect(draining.purchase!.revision).not.toBe(active.purchase!.revision);expect((await homePosts(db,worker)).find(row=>row.id===p.id)?.purchase).toEqual(draining.purchase);
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(worker.assignmentId).run();await expect(postView(db,worker,p.id)).rejects.toMatchObject({code:'POST_NOT_FOUND'});expect((await socialPresentation(db,worker)).purchase).toMatchObject({newWork:false,settleExisting:false,history:false});
});
it('purchasedSocialPresentation preserves current Projects-only included post controls without granting ordinary Work task creation or full People',async()=>{
 const {saveProject,projectSchema}=await import('../../../src/server/modules/projects/public/staff-projects');await socialPackage('projects');vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const project=await saveProject(db,owner,null,projectSchema.parse({title:'Actual included presentation source',memberUserIds:[worker.userId]}),key()),draft=await savePostDraft(db,owner,null,{body:'Actual included Project update',audience:{kind:'company'},sources:[{kind:'staff_project',id:project.id}],purpose:'handover',expectedVersion:1},key()),p=await publishPost(db,owner,draft.id,draft.version,key()),view=await postView(db,worker,p.id);
 expect(view.purchase).toMatchObject({newWork:true,settleExisting:true,history:true,createTask:false});expect(view.body).toBe('Actual included Project update');expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=?').bind(company).all()).results).toEqual([{package_id:'projects'}]);await socialPackage('projects','draining');expect((await postView(db,worker,p.id)).purchase).toMatchObject({newWork:false,settleExisting:true,history:true,createTask:false});
});
it('purchasedSocialPresentation keeps disabled compatibility and source-only no-purchase discovery closed without leaking private authority',async()=>{
 const {socialPresentation}=await import('../../../src/server/modules/collaboration/public/social-home'),p=await post();expect((await postView(db,worker,p.id)).purchase).toBeUndefined();expect((await socialPresentation(db,worker)).purchase).toBeUndefined();vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');expect((await socialPresentation(db,worker)).purchase).toMatchObject({newWork:false,settleExisting:false,history:false});expect(await homePosts(db,worker)).toEqual([]);await expect(postView(db,worker,p.id)).rejects.toMatchObject({code:'POST_NOT_FOUND'});
});

async function privateSocialFile(actor=owner,publish=true,sources:Array<{kind:'staff_project';id:string}>=[]){
 const {seedAttachmentSet}=await import('../../support/helpers/attachment-v2'),{bindCompatibilityAttachmentSet,bindRecordAttachmentSet}=await import('../../../src/server/modules/files/public/attachments/service');vi.stubEnv('ERPX_ATTACHMENTS_V2','true');
 const draft=await savePostDraft(db,actor,null,{body:'Original private Social file source',audience:{kind:'company'},purpose:sources.length?'handover':'update',sources,expectedVersion:1},key()),image=await seedAttachmentSet(db,actor,{targetType:'social_draft',targetId:draft.id,componentKind:'record',componentRef:'legacy-upload-adapter',purposeKey:'post_image',expectedTargetVersion:draft.version,mediaType:'image/png',compatibilityAdapter:true});
 await bindCompatibilityAttachmentSet(db,actor,image.setId,draft.version,key(),{compatibilityAdapter:true});
 const objectKey=(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(image.sourceId).first<string>('object_key'))!,bytes='SOCIAL!!';await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:'image/png'}});
 if(!publish)return {draftId:draft.id,postId:null,image,document:null,bytes,objectKey};
 const draftVersion=(await db.prepare('SELECT version FROM social_post_drafts WHERE id=?').bind(draft.id).first<number>('version'))!,p=await publishPost(db,actor,draft.id,draftVersion,key()),document=await seedAttachmentSet(db,actor,{targetType:'social_post',targetId:p.id,componentKind:'record',purposeKey:sources.length?'handover_file':'thread_file',expectedTargetVersion:p.version});await bindRecordAttachmentSet(db,actor,document.setId,p.version,key());
 const documentKey=(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(document.sourceId).first<string>('object_key'))!;await env.H001_FILES.put(documentKey,'DOCUMENT',{httpMetadata:{contentType:'application/pdf'}});
 return {draftId:draft.id,postId:p.id,image,document,bytes,objectKey};
}
function socialByteStore(read:(key:string)=>void=()=>{}){return {put:(...args:Parameters<typeof env.H001_FILES.put>)=>env.H001_FILES.put(...args),head:(...args:Parameters<typeof env.H001_FILES.head>)=>env.H001_FILES.head(...args),delete:(...args:Parameters<typeof env.H001_FILES.delete>)=>env.H001_FILES.delete(...args),list:(...args:Parameters<typeof env.H001_FILES.list>)=>env.H001_FILES.list(...args),get:async(...args:Parameters<typeof env.H001_FILES.get>)=>{read(args[0]);return env.H001_FILES.get(...args);}};}
it('purchasedSocialBytes retains exact original private image and document bytes through disabled active draining normal inactive and Legal hold',async()=>{
 const {attachmentContent}=await import('../../../src/server/modules/files/public/attachments/service'),f=await privateSocialFile();expect(f.document).not.toBeNull();expect(await (await attachmentContent(db,env.H001_FILES,worker,f.image.linkId,'original')).object.text()).toBe(f.bytes);
 vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');let reads=0;await expect(attachmentContent(db,socialByteStore(()=>{reads++;}),worker,f.image.linkId,'original')).rejects.toMatchObject({status:404,code:'POST_NOT_FOUND'});expect(reads).toBe(0);await socialPackage('work');await socialPackage();await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(f.image.sourceId).run();
 for(const state of ['active','draining'] as const){if(state==='draining')await socialPackage('work-social','draining');expect(await (await attachmentContent(db,env.H001_FILES,worker,f.image.linkId,'original')).object.text()).toBe(f.bytes);expect(await (await attachmentContent(db,env.H001_FILES,worker,f.document!.linkId,'original')).object.text()).toBe('DOCUMENT');}
 await socialInactive();expect(await (await attachmentContent(db,env.H001_FILES,worker,f.image.linkId,'original')).object.text()).toBe(f.bytes);expect(await db.prepare('SELECT legal_hold,revoked_at FROM attachments WHERE id=?').bind(f.image.sourceId).first()).toEqual({legal_hold:1,revoked_at:null});expect(owner.modules).toEqual([]);
});
it('purchasedSocialBytes preserves original current and already-invalid denial audits without bytes and rolls back a changing invalid authority denial',async()=>{
 const {attachmentContent}=await import('../../../src/server/modules/files/public/attachments/service'),f=await privateSocialFile(owner,false);let reads=0;const files=socialByteStore(()=>{reads++;});await expect(attachmentContent(db,files,worker,f.image.linkId,'original')).rejects.toMatchObject({status:403,code:'DRAFT_LOCKED'});expect(reads).toBe(0);expect(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='denied' AND reason='DRAFT_LOCKED'").bind(f.image.linkId).first('n')).toBe(1);
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(owner.assignmentId).run();await expect(attachmentContent(db,files,owner,f.image.linkId,'original')).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});expect(reads).toBe(0);const before=await db.prepare('SELECT count(*) n FROM attachment_access_events WHERE link_id=?').bind(f.image.linkId).first<number>('n'),fn=`social_byte_denial_${key().replaceAll('-','')}`;
 await db.exec(`CREATE SEQUENCE ${fn}_fired;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_fired');UPDATE assignments SET version=version+1 WHERE id='${owner.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${f.image.linkId}' AND NEW.outcome='denied') EXECUTE FUNCTION ${fn}()`);
 try{await expect(attachmentContent(db,files,owner,f.image.linkId,'original')).rejects.toMatchObject({code:'SOCIAL_FILE_READ_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${fn}_fired`).first('is_called')).toBe(true);}finally{await db.exec(`DROP TRIGGER ${fn} ON attachment_access_events;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_fired`);}expect(reads).toBe(0);expect(await db.prepare('SELECT count(*) n FROM attachment_access_events WHERE link_id=?').bind(f.image.linkId).first('n')).toBe(before);expect(await db.prepare('SELECT version,revoked_at FROM assignments WHERE id=?').bind(owner.assignmentId).first()).toEqual({version:2,revoked_at:1});
});
it('purchasedSocialBytes buffers lazily and rolls back allowed audits with actual draft post File or current IAM deferred changes before releasing bytes',async()=>{
 const {attachmentContent}=await import('../../../src/server/modules/files/public/attachments/service');await socialPackage('work');await socialPackage();vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');const f=await privateSocialFile(),postId=f.postId!,document=f.document!;
 for(const kind of ['draft','post','file','iam'] as const){const link=kind==='draft'?f.image.linkId:document.linkId,fn=`social_byte_${kind}_${key().replaceAll('-','')}`,effect=kind==='draft'?`UPDATE social_post_drafts SET body='Unreviewed lazy source' WHERE id='${f.draftId}'`:kind==='post'?`UPDATE social_posts SET body='Unreviewed lazy source' WHERE id='${postId}'`:kind==='file'?`UPDATE attachments SET revoked_at=1 WHERE id='${document.sourceId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${worker.assignmentId}'`;
 await db.exec(`CREATE SEQUENCE ${fn}_fired;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_fired');${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${link}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fn}()`);
 let consumed=0;const base=socialByteStore(),files={...base,get:async(...args:Parameters<typeof base.get>)=>{const object=await base.get(...args);return object?{...object,arrayBuffer:async()=>{consumed++;return object.arrayBuffer();}}:null;}};
 try{await expect(attachmentContent(db,files,worker,link,'original')).rejects.toMatchObject({code:kind==='iam'?'ATTACHMENT_FILE_READ_CHANGED':'SOCIAL_FILE_READ_CHANGED'});expect(consumed).toBe(1);expect(await db.prepare(`SELECT is_called FROM ${fn}_fired`).first('is_called')).toBe(true);}finally{await db.exec(`DROP TRIGGER ${fn} ON attachment_access_events;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_fired`);}
 expect(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(link).first('n')).toBe(0);expect(await db.prepare('SELECT body FROM social_posts WHERE id=?').bind(postId).first('body')).toBe('Original private Social file source');expect(await db.prepare('SELECT body FROM social_post_drafts WHERE id=?').bind(f.draftId).first('body')).toBe('Original private Social file source');expect(await db.prepare('SELECT revoked_at FROM attachments WHERE id=?').bind(document.sourceId).first('revoked_at')).toBeNull();expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(worker.assignmentId).first()).toEqual({revoked_at:null,version:1});
 }
 expect(await (await attachmentContent(db,env.H001_FILES,worker,document.linkId,'original')).object.text()).toBe('DOCUMENT');
});
it('purchasedSocialBytes preserves Projects-only real source authority and refuses both late source membership changes and arbitrary supplied snapshots',async()=>{
 const {attachmentContent}=await import('../../../src/server/modules/files/public/attachments/service'),{saveProject,projectSchema}=await import('../../../src/server/modules/projects/public/staff-projects');await socialPackage('projects');vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const project=await saveProject(db,owner,null,projectSchema.parse({title:'Original private included Project',memberUserIds:[worker.userId]}),key()),f=await privateSocialFile(owner,true,[{kind:'staff_project',id:project.id}]),document=f.document!,fn=`social_byte_project_${key().replaceAll('-','')}`;
 expect(await (await attachmentContent(db,env.H001_FILES,worker,document.linkId,'original')).object.text()).toBe('DOCUMENT');await expect(db.transaction(async tx=>{await tx.prepare('SELECT 1').first();return attachmentContent(tx,env.H001_FILES,worker,document.linkId,'original');})).rejects.toMatchObject({code:'SOCIAL_FILE_READ_TRANSACTION_REQUIRED'});
 await db.exec(`CREATE SEQUENCE ${fn}_fired;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_fired');DELETE FROM staff_project_members WHERE project_id='${project.id}' AND user_id='${worker.userId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${document.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fn}()`);
 try{await expect(attachmentContent(db,env.H001_FILES,worker,document.linkId,'original')).rejects.toMatchObject({code:'POST_NOT_FOUND'});expect(await db.prepare(`SELECT is_called FROM ${fn}_fired`).first('is_called')).toBe(true);}finally{await db.exec(`DROP TRIGGER ${fn} ON attachment_access_events;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_fired`);}
 expect(await db.prepare('SELECT count(*) n FROM staff_project_members WHERE project_id=? AND user_id=?').bind(project.id,worker.userId).first('n')).toBe(1);expect(await (await attachmentContent(db,env.H001_FILES,worker,document.linkId,'original')).object.text()).toBe('DOCUMENT');expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=?').bind(company).all()).results).toEqual([{package_id:'projects'}]);
});
it('purchasedSocialBytes finishes two Companies original source-writing access audits without hidden deadlock retries or a lazy byte escape',async()=>{
 const {expenseFixture}=await import('../../support/helpers/expense-fixture'),other=await expenseFixture(),first=await privateSocialFile(owner,false),second=await privateSocialFile(other.owner,false),{attachmentContent}=await import('../../../src/server/modules/files/public/attachments/service'),{measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics'),fn=`social_byte_companies_${key().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.08);UPDATE social_post_drafts SET body=body WHERE company_id=NEW.company_id AND id IN('${first.draftId}','${second.draftId}');RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id IN('${first.image.linkId}','${second.image.linkId}') AND NEW.outcome='allowed') EXECUTE FUNCTION ${fn}()`);
 try{const observed=await measureReads(()=>Promise.all([attachmentContent(db,env.H001_FILES,owner,first.image.linkId,'original'),attachmentContent(db,env.H001_FILES,other.owner,second.image.linkId,'original')]));expect(await Promise.all(observed.result.map(value=>value.object.text()))).toEqual([first.bytes,second.bytes]);expect(observed.metrics.deadlockFailures??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${fn} ON attachment_access_events;DROP FUNCTION ${fn}()`);}
 expect(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id IN (?,?) AND outcome='allowed'").bind(first.image.linkId,second.image.linkId).first('n')).toBe(2);
});
it('purchasedSocialBytes waits for a normal committed Project membership writer before its fresh source snapshot instead of returning stale included private bytes',async()=>{
 const {attachmentContent}=await import('../../../src/server/modules/files/public/attachments/service'),{saveProject,projectSchema}=await import('../../../src/server/modules/projects/public/staff-projects');await socialPackage('projects');vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');const project=await saveProject(db,owner,null,projectSchema.parse({title:'Actual committed private source writer',memberUserIds:[worker.userId]}),key()),f=await privateSocialFile(owner,true,[{kind:'staff_project',id:project.id}]),document=f.document!,command=key(),fn=`social_byte_writer_${key().replaceAll('-','')}`;
 await db.exec(`CREATE SEQUENCE ${fn}_fired;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_fired');PERFORM pg_sleep(0.7);RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fn}()`);
 let writer:Promise<{id:string;version:number}>|undefined,reads=0;
 try{writer=saveProject(db,owner,project.id,projectSchema.parse({title:'Actual committed private source writer',memberUserIds:[],expectedVersion:1}),command);const deadline=Date.now()+5000;while(!await db.prepare(`SELECT is_called FROM ${fn}_fired`).first<boolean>('is_called')){if(Date.now()>deadline)throw Error('Actual native Project writer did not reach its durable deferred receipt');await new Promise(resolve=>setTimeout(resolve,5));}
 await expect(attachmentContent(db,socialByteStore(()=>{reads++;}),worker,document.linkId,'original')).rejects.toMatchObject({status:404,code:'POST_NOT_FOUND'});expect((await writer).version).toBe(2);expect(reads).toBe(0);expect(await db.prepare('SELECT count(*) n FROM staff_project_members WHERE project_id=? AND user_id=?').bind(project.id,worker.userId).first('n')).toBe(0);
 }finally{await writer?.catch(()=>{});await db.exec(`DROP TRIGGER ${fn} ON command_receipts;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_fired`);}
 await saveProject(db,owner,project.id,projectSchema.parse({title:'Actual committed private source writer',memberUserIds:[worker.userId],expectedVersion:2}),key());expect(await (await attachmentContent(db,env.H001_FILES,worker,document.linkId,'original')).object.text()).toBe('DOCUMENT');expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=?').bind(company).all()).results).toEqual([{package_id:'projects'}]);
});

it('purchasedSocialBytes preserves only original quarantined and unavailable-storage denial events with exact policy source and no byte delivery',async()=>{
 const {attachmentContent}=await import('../../../src/server/modules/files/public/attachments/service'),{seedAttachmentSet}=await import('../../support/helpers/attachment-v2'),f=await privateSocialFile(),postVersion=(await db.prepare('SELECT version FROM social_posts WHERE id=?').bind(f.postId).first<number>('version'))!,quarantined=await seedAttachmentSet(db,owner,{targetType:'social_post',targetId:f.postId!,componentKind:'record',purposeKey:'thread_file',expectedTargetVersion:postVersion,sourceState:'quarantined'});let reads=0;
 await expect(attachmentContent(db,socialByteStore(()=>{reads++;}),owner,quarantined.linkId,'original')).rejects.toMatchObject({status:423,code:'ATTACHMENT_NOT_AVAILABLE'});expect(reads).toBe(0);expect(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='denied' AND reason='ATTACHMENT_NOT_AVAILABLE'").bind(quarantined.linkId).first('n')).toBe(1);
 const base=socialByteStore(),unavailable={...base,get:async()=>{reads++;return null;}};await expect(attachmentContent(db,unavailable,worker,f.document!.linkId,'original')).rejects.toMatchObject({status:503,code:'FILE_UNAVAILABLE'});expect(reads).toBe(1);expect(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='denied' AND reason='FILE_UNAVAILABLE'").bind(f.document!.linkId).first('n')).toBe(1);
 const fn=`social_byte_unavailable_${key().replaceAll('-','')}`;await db.exec(`CREATE SEQUENCE ${fn}_fired;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_fired');UPDATE social_posts SET body='Unreviewed denial source' WHERE id='${f.postId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${f.document!.linkId}' AND NEW.outcome='denied') EXECUTE FUNCTION ${fn}()`);
 try{await expect(attachmentContent(db,unavailable,worker,f.document!.linkId,'original')).rejects.toMatchObject({code:'SOCIAL_FILE_READ_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${fn}_fired`).first('is_called')).toBe(true);}finally{await db.exec(`DROP TRIGGER ${fn} ON attachment_access_events;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_fired`);}
 expect(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='denied'").bind(f.document!.linkId).first('n')).toBe(1);expect(await db.prepare('SELECT body FROM social_posts WHERE id=?').bind(f.postId).first('body')).toBe('Original private Social file source');
});

it('purchasedHandoverReceipt seeds actual Projects-only participants and confirms the rendered version without Work or full People while current source removal stays private',async()=>{
 const {saveProject,projectSchema}=await import('../../../src/server/modules/projects/public/staff-projects'),{handoverAcknowledgements}=await import('../../../src/server/modules/operations/public/handover-acknowledgement-read'),{handoverAcknowledgement,acknowledgeHandoverVersion}=await import('../../../src/server/modules/operations/public/handover');await socialPackage('projects');vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const project=await saveProject(db,owner,null,projectSchema.parse({title:'Actual included rendered handover receipt',memberUserIds:[worker.userId]}),key()),draft=await savePostDraft(db,owner,null,{body:'Read this actual Project handover before confirming',audience:{kind:'participants',userIds:[worker.userId]},sources:[{kind:'staff_project',id:project.id}],purpose:'handover',expectedVersion:1},key()),published=await publishPost(db,owner,draft.id,draft.version,key()),initial=await handoverAcknowledgement(db,worker,published.id);
 expect(initial).toMatchObject({postVersion:1,recipientCount:1,acknowledgedCount:0,canAcknowledge:true,acknowledged:false});expect((await postView(db,worker,published.id)).handoverReceipt).toEqual(initial);expect(await handoverAcknowledgements(db,worker,[published.id])).toEqual([initial]);expect((await postView(db,owner,published.id)).handoverReceipt).toMatchObject({canAcknowledge:false});expect(await handoverAcknowledgements(db,reviewer,[published.id])).toEqual([]);
 const command=key();expect(await acknowledgeHandoverVersion(db,worker,published.id,command,1)).toMatchObject({postId:published.id,postVersion:1,acknowledged:true,acknowledgedCount:1});expect((await postView(db,worker,published.id)).handoverReceipt).toMatchObject({postVersion:1,acknowledged:true,acknowledgedCount:1});expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=?').bind(company).all()).results).toEqual([{package_id:'projects'}]);expect(worker.modules).toEqual([]);
 await saveProject(db,owner,project.id,projectSchema.parse({title:'Actual included rendered handover receipt',memberUserIds:[],expectedVersion:1}),key());expect(await handoverAcknowledgements(db,worker,[published.id])).toEqual([]);await expect(acknowledgeHandoverVersion(db,worker,published.id,command,1)).rejects.toMatchObject({code:'POST_NOT_FOUND'});expect(await db.prepare('SELECT count(*) n FROM social_handover_acknowledgements WHERE post_id=?').bind(published.id).first('n')).toBe(1);
});
it('includedProductPost preserves strict Projects entry with all modes disabled and native Project-only new-work settlement and source privacy when only Projects enforcement is enabled',async()=>{
 const {saveProject,projectSchema}=await import('../../../src/server/modules/projects/public/staff-projects'),{socialPostProductSourceEntry}=await import('../../../src/server/modules/collaboration/public/product-entry'),project=await saveProject(db,owner,null,projectSchema.parse({title:'Actual Project product post source',memberUserIds:[worker.userId]}),key()),draft=await savePostDraft(db,owner,null,{body:'Actual Project-only ordinary update',audience:{kind:'company'},sources:[{kind:'staff_project',id:project.id}],expectedVersion:1},key()),published=await publishPost(db,owner,draft.id,draft.version,key()),scope={companyId:worker.companyId,branchId:worker.branchId,departmentId:worker.departmentId,userId:worker.userId,assignmentId:worker.assignmentId,assignmentVersion:1,role:worker.role};
 expect(await socialPostProductSourceEntry(db,scope,{kind:'post',id:published.id})).toBeNull();await socialPackage('projects');expect(await socialPostProductSourceEntry(db,scope,{kind:'post',id:published.id})).toEqual({purchasePurpose:'history'});vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');expect((await postView(db,worker,published.id)).purchase).toMatchObject({newWork:true,settleExisting:true,history:true});await socialPackage('projects','draining');expect((await postView(db,worker,published.id)).purchase).toMatchObject({newWork:false,settleExisting:true,history:true});await expect(postAction(db,owner,published.id,{action:'edit',expectedVersion:1,body:'A new unlicensed Project amendment'},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});await postAction(db,worker,published.id,{action:'comment',body:'Actual Project settlement without full Work'},key());await db.prepare('DELETE FROM staff_project_members WHERE project_id=? AND user_id=?').bind(project.id,worker.userId).run();expect(await socialPostProductSourceEntry(db,scope,{kind:'post',id:published.id})).toBeNull();await expect(postView(db,worker,published.id)).rejects.toMatchObject({code:'POST_NOT_FOUND'});expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=?').bind(company).all()).results).toEqual([{package_id:'projects'}]);expect(worker.modules).toEqual([]);
});


it('projectAudienceOptions hides actual unpurchased Projects from the normal Work Social composer', async () => {
  const { saveProject, getProject, projectSchema } = await import('@/server/modules/projects/public/staff-projects');
  const { socialAudienceOptions } = await import('@/server/modules/collaboration/public/social-home');
  const project = await saveProject(db, owner, null, projectSchema.parse({ title: 'Private Project audience requires its actual owner history', memberUserIds: [worker.userId] }), key());
  const legacyDraft = await savePostDraft(db, worker, null, { body: 'Original Project audience before the owner purchase boundary', audience: { kind: 'project', id: project.id }, expectedVersion: 1 }, key());
  const legacyPost = await publishPost(db, worker, legacyDraft.id, legacyDraft.version, key());
  await socialPackage('work');
  await socialPackage();
  vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT', 'disabled');
  vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT', 'enforced');
  vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT', 'enforced');
  await expect(getProject(db, worker, project.id)).rejects.toMatchObject({ code: 'PACKAGE_REQUIRED' });
  const options = await socialAudienceOptions(db, worker);
  expect(options.projects).toEqual([]);
  expect(options.audiences.some(audience => audience.kind === 'project')).toBe(false);
  await expect(postView(db, worker, legacyPost.id)).rejects.toMatchObject({ code: 'POST_NOT_FOUND' });
  vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT', 'enforced');
  await expect(postView(db, worker, legacyPost.id)).rejects.toMatchObject({ code: 'POST_NOT_FOUND' });
});

it('projectAudienceOptions also refuses an actual Project audience draft command when the owner purchase is absent', async () => {
  const { saveProject, projectSchema } = await import('@/server/modules/projects/public/staff-projects');
  const project = await saveProject(db, owner, null, projectSchema.parse({ title: 'Actual Project audience command source', memberUserIds: [worker.userId] }), key());
  const legacyDraft = await savePostDraft(db, worker, null, { body: 'An existing draft retains its actual audience identity', audience: { kind: 'project', id: project.id }, expectedVersion: 1 }, key());
  await socialPackage('work');
  await socialPackage();
  vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT', 'enforced');
  vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT', 'enforced');
  vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT', 'enforced');
  await expect(savePostDraft(db, worker, null, { body: 'An audience ID cannot substitute for its Project history', audience: { kind: 'project', id: project.id }, expectedVersion: 1 }, key()))
    .rejects.toMatchObject({ status: 403, code: 'PACKAGE_REQUIRED' });
  await expect(publishPost(db, worker, legacyDraft.id, legacyDraft.version, key())).rejects.toMatchObject({ status: 403, code: 'PACKAGE_REQUIRED' });
});

it('projectAudienceOptions preserves Projects-only membership and expired purchased history without full People or Work', async () => {
  const { saveProject, projectSchema } = await import('@/server/modules/projects/public/staff-projects');
  const { socialAudienceOptions } = await import('@/server/modules/collaboration/public/social-home');
  const { readCompanyPackageGrants } = await import('@/server/modules/commercial/public/package-grants');
  await socialPackage('projects');
  vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT', 'enforced');
  vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT', 'enforced');
  vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT', 'enforced');
  const project = await saveProject(db, owner, null, projectSchema.parse({ title: 'Actual included Project audience', memberUserIds: [worker.userId] }), key());
  const expected = [{ id: project.id, title: 'Actual included Project audience' }];
  expect((await socialAudienceOptions(db, worker)).projects).toEqual(expected);
  expect((await socialAudienceOptions(db, reviewer)).projects).toEqual([]);
  const draft = await savePostDraft(db, owner, null, { body: 'Actual Project-only source conversation', audience: { kind: 'project', id: project.id }, sources: [{ kind: 'staff_project', id: project.id }], expectedVersion: 1 }, key());
  const publishKey = key(), published = await publishPost(db, owner, draft.id, draft.version, publishKey);
  expect((await postView(db, worker, published.id)).body).toBe('Actual Project-only source conversation');
  await socialPackage('projects', 'draining');
  expect((await socialAudienceOptions(db, worker)).projects).toEqual(expected);
  await socialPackage('projects', 'active', Date.now() - 1000);
  expect((await socialAudienceOptions(db, worker)).projects).toEqual(expected);
  expect((await postView(db, worker, published.id)).body).toBe('Actual Project-only source conversation');
  expect(await publishPost(db, owner, draft.id, draft.version, publishKey)).toEqual(published);
  await postAction(db, worker, published.id, { action: 'comment', body: 'Current member settles the retained Project conversation' }, key());
  expect((await readCompanyPackageGrants(db, company)).map(row => row.packageId)).toEqual(['projects']);
  await socialPackage('projects');
  await saveProject(db, owner, project.id, projectSchema.parse({ title: 'Actual included Project audience', memberUserIds: [], expectedVersion: 1 }), key());
  expect((await socialAudienceOptions(db, worker)).projects).toEqual([]);
  await expect(postView(db, worker, published.id)).rejects.toMatchObject({ code: 'POST_NOT_FOUND' });
});

it('projectAudienceOptions retains legacy Beta scope and refreshes current role and release before cached choices', async () => {
  const { saveProject, projectSchema } = await import('@/server/modules/projects/public/staff-projects');
  const { socialAudienceOptions } = await import('@/server/modules/collaboration/public/social-home');
  const project = await saveProject(db, owner, null, projectSchema.parse({ title: 'Original Beta Project membership audience', memberUserIds: [worker.userId] }), key());
  const expected = [{ id: project.id, title: 'Original Beta Project membership audience' }];
  for (const mode of ['disabled', 'shadow'] as const) {
    vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT', mode);
    expect((await socialAudienceOptions(db, worker)).projects).toEqual(expected);
  }
  await socialPackage('projects');
  vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT', 'enforced');
  expect((await socialAudienceOptions(db, worker)).projects).toEqual(expected);
  vi.stubEnv('NODE_ENV', 'production');
  expect((await socialAudienceOptions(db, worker)).projects).toEqual([]);
  await expect(savePostDraft(db, worker, null, { body: 'No Project release cannot authorize an audience', audience: { kind: 'project', id: project.id }, expectedVersion: 1 }, key()))
    .rejects.toMatchObject({ code: 'PROJECT_AUDIENCE_FORBIDDEN' });
  vi.stubEnv('NODE_ENV', 'test');
  expect((await socialAudienceOptions(db, worker)).projects).toEqual(expected);
  expect((await socialAudienceOptions(db, owner)).projects).toEqual(expected);
  await db.prepare('UPDATE assignments SET version=version+1 WHERE id=?').bind(owner.assignmentId).run();
  expect((await socialAudienceOptions(db, owner)).projects).toEqual([]);
  await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(), worker.assignmentId).run();
  expect((await socialAudienceOptions(db, worker)).projects).toEqual([]);
});

it('projectAudienceOptions rolls back a real Project audience source change after the native Social receipt', async () => {
  const { saveProject, projectSchema } = await import('@/server/modules/projects/public/staff-projects');
  await socialPackage('projects');
  vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT', 'enforced');
  vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT', 'enforced');
  vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT', 'enforced');
  const project = await saveProject(db, owner, null, projectSchema.parse({ title: 'Original reviewed Project audience source', memberUserIds: [worker.userId] }), key());
  const command = key(), draftId = `post-draft-${command}`, fault = `project_audience_${key().replaceAll('-', '')}`;
  const before = await db.prepare('SELECT title,version FROM staff_projects WHERE id=?').bind(project.id).first();
  await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='${command}' THEN PERFORM nextval('${fault}');UPDATE staff_projects SET title='Deferred unreviewed Project audience',version=version+1 WHERE id='${project.id}';END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ${fault}()`);
  try {
    await expect(savePostDraft(db, owner, null, { body: 'Actual included source cannot change behind the completed draft', audience: { kind: 'project', id: project.id }, sources: [{ kind: 'staff_project', id: project.id }], expectedVersion: 1 }, command))
      .rejects.toMatchObject({ code: 'PROJECT_AUDIENCE_CHANGED' });
    expect(await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called')).toBe(true);
    expect(await db.prepare('SELECT title,version FROM staff_projects WHERE id=?').bind(project.id).first()).toEqual(before);
    expect(await db.prepare('SELECT 1 FROM social_post_drafts WHERE id=?').bind(draftId).first()).toBeNull();
    for (const table of ['command_receipts', 'audit_events', 'outbox_events']) expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();
  } finally {
    await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);
  }
});

// Native scan provider fixture only; all metadata, bytes, binding and business
// state below still come through original Files/Social/Projects commands.
vi.mock('../../../src/server/modules/files/public/attachments/scanner',()=>({attachmentScannerReadiness:async()=>({enabled:true,ready:true,engine:'ClamAV 1.5.4/native-fixture',definitionUpdatedAt:Date.now(),error:null}),scanAttachmentBytes:async()=>({clean:true,signature:null,engine:'ClamAV 1.5.4/native-fixture',definitionUpdatedAt:Date.now()})}));
async function projectCleanupImage(actor:WorkContext,draftId:string,version:number,bind=true){
 const files=await import('../../../src/server/modules/files/public/attachments/service');
 const bytes=Uint8Array.from(Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=','base64'));
 const set=await files.createAttachmentSet(db,actor,{targetType:'social_draft',targetId:draftId,componentKind:'record',componentRef:'legacy-upload-adapter',purposeKey:'post_image',expectedTargetVersion:version},key());
 const reservation=await files.reserveAttachmentFile(db,actor,set.id,{filename:'pending-project-image.png',mediaType:'image/png',bytes:bytes.byteLength},key());
 const uploaded=await files.completeAttachmentUpload(db,env.H001_FILES,actor,reservation.uploadIntentId,bytes.buffer);
 expect(await files.drainAttachmentScans(db,env.H001_FILES,uploaded.sourceId)).toBe(true);
 const binding=bind?await files.bindCompatibilityAttachmentSet(db,actor,set.id,version,key()):null;
 const stored=await db.prepare('SELECT object_key,checksum_sha256 FROM attachments WHERE id=?').bind(uploaded.sourceId).first<{object_key:string;checksum_sha256:string}>();if(!stored)throw Error('Actual original uploaded source is required');
 return {...reservation,...uploaded,setId:set.id,ownerVersion:binding?.ownerVersion??version,objectKey:stored.object_key,checksum:stored.checksum_sha256};
}
async function projectCleanupFixture(mode:'disabled'|'enforced'){
 const native=await import('../../../src/server/modules/projects/public/staff-projects');
 await socialPackage('projects');vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT',mode);vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_ATTACHMENTS_V2','true');vi.stubEnv('ERPX_ATTACHMENT_UPLOAD_FREEZE','false');
 await freshIam(worker);await freshIam(reviewer);
 const project=await native.saveProject(db,owner,null,native.projectSchema.parse({title:'Actual author-owned pending Project uploads',memberUserIds:[worker.userId]}),key());
 const draft=await savePostDraft(db,worker,null,{body:'Private Project source must remain inaccessible after membership ends',audience:{kind:'project',id:project.id},sources:[{kind:'staff_project',id:project.id}],expectedVersion:1},key()),published=await publishPost(db,worker,draft.id,draft.version,key());
 const comment=await createCommentUploadDraft(db,worker,published.id,key()),edit=await createPostEditUploadDraft(db,worker,published.id,1,key());
 const commentFile=await projectCleanupImage(worker,comment.id,1),editFile=await projectCleanupImage(worker,edit.id,edit.version,false);
 return {project,published,comment,edit,commentFile,editFile,removeMember:()=>native.saveProject(db,owner,project.id,native.projectSchema.parse({title:'Actual author-owned pending Project uploads',memberUserIds:[],expectedVersion:1}),key())};
}
async function projectCleanupState(ids:string[],command?:string){return db.prepare(`SELECT jsonb_build_object('drafts',(SELECT jsonb_agg(to_jsonb(d) ORDER BY d.id) FROM social_post_drafts d WHERE d.id IN(SELECT jsonb_array_elements_text(?::jsonb))),'files',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM attachments a WHERE a.owner_type='social_draft' AND a.owner_id IN(SELECT jsonb_array_elements_text(?::jsonb))),'uploads',(SELECT jsonb_agg(to_jsonb(u) ORDER BY u.id) FROM upload_intents u WHERE u.owner_type='social_draft' AND u.owner_id IN(SELECT jsonb_array_elements_text(?::jsonb))),'sets',(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.id) FROM attachment_sets s WHERE s.target_type='social_draft' AND s.target_id IN(SELECT jsonb_array_elements_text(?::jsonb))),'links',(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE s.target_type='social_draft' AND s.target_id IN(SELECT jsonb_array_elements_text(?::jsonb))),'events',(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM attachment_events e JOIN attachment_sets s ON s.id=e.set_id WHERE s.target_type='social_draft' AND s.target_id IN(SELECT jsonb_array_elements_text(?::jsonb))),'assignment',(SELECT to_jsonb(a) FROM assignments a WHERE a.id=?),'receipt',(SELECT to_jsonb(c) FROM command_receipts c WHERE c.command_id=?),'audit',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM audit_events a WHERE a.command_id=?),'outbox',(SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM outbox_events o WHERE o.command_id=?))::text image`).bind(JSON.stringify(ids),JSON.stringify(ids),JSON.stringify(ids),JSON.stringify(ids),JSON.stringify(ids),JSON.stringify(ids),worker.assignmentId,command??'',command??'',command??'').first<string>('image');}
for(const mode of ['disabled','enforced'] as const)it(`projectUploadCleanup preserves actual pending author File closure after member loss with Work ${mode}`,async()=>{
 const {cancelCommentUploadDraft,cancelPostEditUploadDraft}=await import('../../../src/server/modules/collaboration/public/social-home'),f=await projectCleanupFixture(mode),consumed=await createCommentUploadDraft(db,worker,f.published.id,key());
 const consumedFile=await projectCleanupImage(worker,consumed.id,1),consumedRich=plainRichContent('Normally consumed comment upload');consumedRich.blocks.push({type:'image',slotId:key(),attachmentId:consumedFile.sourceId,name:'pending-project-image.png'});await postAction(db,worker,f.published.id,{action:'comment',body:'Normally consumed comment upload',richContent:consumedRich,commentDraftId:consumed.id},key());
 const consumedCancelKey=key(),beforeConsumed=await projectCleanupState([consumed.id],consumedCancelKey);await expect(cancelCommentUploadDraft(db,worker,consumed.id,consumedCancelKey)).rejects.toMatchObject({code:'UPLOAD_DRAFT_CONSUMED'});expect(await projectCleanupState([consumed.id],consumedCancelKey)).toBe(beforeConsumed);
 await f.removeMember();if(mode==='enforced')await socialPackage('projects','draining');await expect(postView(db,worker,f.published.id)).rejects.toMatchObject({code:'POST_NOT_FOUND'});await expect(createCommentUploadDraft(db,worker,f.published.id,key())).rejects.toMatchObject({status:403});
 const foreignCancelKey=key(),foreignBefore=await projectCleanupState([f.comment.id,f.edit.id],foreignCancelKey);await expect(cancelCommentUploadDraft(db,reviewer,f.comment.id,foreignCancelKey)).rejects.toMatchObject({code:'DRAFT_LOCKED'});expect(await projectCleanupState([f.comment.id,f.edit.id],foreignCancelKey)).toBe(foreignBefore);
 const commentKey=key(),editKey=key(),parentBefore=await db.prepare('SELECT to_jsonb(p)::text image FROM social_posts p WHERE id=?').bind(f.published.id).first<string>('image');
 expect(await cancelCommentUploadDraft(db,worker,f.comment.id,commentKey)).toEqual({id:f.comment.id});expect(await cancelPostEditUploadDraft(db,worker,f.edit.id,editKey)).toEqual({id:f.edit.id});
 const completed=await projectCleanupState([f.comment.id,f.edit.id]);expect(await cancelCommentUploadDraft(db,worker,f.comment.id,commentKey)).toEqual({id:f.comment.id});expect(await cancelPostEditUploadDraft(db,worker,f.edit.id,editKey)).toEqual({id:f.edit.id});expect(await projectCleanupState([f.comment.id,f.edit.id])).toBe(completed);
 for(const file of [f.commentFile,f.editFile]){expect(await db.prepare('SELECT revoked_at FROM attachments WHERE id=?').bind(file.sourceId).first('revoked_at')).not.toBeNull();expect(await db.prepare('SELECT status FROM upload_intents WHERE id=?').bind(file.uploadIntentId).first('status')).toBe('expired');const bytes=await env.H001_FILES.get(file.objectKey);expect(bytes).not.toBeNull();const {sha256}=await import('../../../src/server/modules/runtime/public/crypto');expect(await sha256(await bytes!.arrayBuffer())).toBe(file.checksum);}
 expect(await db.prepare('SELECT state FROM attachment_sets WHERE id=?').bind(f.commentFile.setId).first('state')).toBe('bound');expect(await db.prepare('SELECT state FROM attachment_sets WHERE id=?').bind(f.editFile.setId).first('state')).toBe('draft');expect(await db.prepare('SELECT to_jsonb(p)::text image FROM social_posts p WHERE id=?').bind(f.published.id).first('image')).toBe(parentBefore);
 await expect(postView(db,worker,f.published.id)).rejects.toMatchObject({code:'POST_NOT_FOUND'});await expect(socialUploadOwner(db,worker,f.comment.id,true)).rejects.toMatchObject({code:'DRAFT_NOT_FOUND'});expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=? ORDER BY package_id').bind(company).all()).results).toEqual([{package_id:'projects'}]);expect(worker.modules).toEqual([]);
});
it('projectUploadCleanup rolls back exact native Files and current authority faults then retries the original command keys',async()=>{
 const {cancelCommentUploadDraft,cancelPostEditUploadDraft}=await import('../../../src/server/modules/collaboration/public/social-home'),f=await projectCleanupFixture('enforced');await f.removeMember();
 for(const kind of ['authority','file'] as const){const command=key(),fault=`social_upload_cleanup_${key().replaceAll('-','')}`,draft=kind==='authority'?f.comment:f.edit,cancel=kind==='authority'?cancelCommentUploadDraft:cancelPostEditUploadDraft,before=await projectCleanupState([f.comment.id,f.edit.id],command);
  const mutation=kind==='authority'?`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${worker.assignmentId}';`:`UPDATE upload_intents SET updated_at=updated_at+1 WHERE id='${f.editFile.uploadIntentId}';`;
  await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='${command}' THEN PERFORM nextval('${fault}');${mutation}END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ${fault}()`);
  try{await expect(cancel(db,worker,draft.id,command)).rejects.toMatchObject({code:kind==='authority'?'WORK_SOCIAL_AUTHORITY_CHANGED':'SOCIAL_UPLOAD_CLEANUP_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called')).toBe(true);expect(await projectCleanupState([f.comment.id,f.edit.id],command)).toBe(before);}
  finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
  expect(await cancel(db,worker,draft.id,command)).toEqual({id:draft.id});expect(await cancel(db,worker,draft.id,command)).toEqual({id:draft.id});
 }
});

it('projectUploadCleanup waits for the actual normal Work selector before its first source snapshot without retry masking',async()=>{
 const f=await projectCleanupFixture('enforced');await f.removeMember();
 const {cancelCommentUploadDraft}=await import('../../../src/server/modules/collaboration/public/social-home'),{measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics'),authority=reviewer.authority;
 if(!authority)throw Error('Actual independently selected native reviewer context required');
 const original=await db.prepare('SELECT revision,expires_at FROM active_work_contexts WHERE session_id=? AND company_id=? AND user_id=?').bind(authority.sessionId,company,reviewer.userId).first<{revision:number;expires_at:number}>();if(!original)throw Error('Actual native selected context required');
 const fault=`social_select_${key().replaceAll('-','')}`,command=key(),parent=await db.prepare('SELECT to_jsonb(p)::text image FROM social_posts p WHERE id=?').bind(f.published.id).first('image');
 let release=()=>{},ready=()=>{};const gateRelease=new Promise<void>(resolve=>{release=resolve;}),gateReady=new Promise<void>(resolve=>{ready=resolve;});
 await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');PERFORM pg_advisory_xact_lock(hashtextextended('${fault}',0));RETURN NEW;END $$;CREATE TRIGGER ${fault} AFTER UPDATE ON active_work_contexts FOR EACH ROW WHEN(NEW.session_id='${authority.sessionId}' AND NEW.company_id='${company}') EXECUTE FUNCTION ${fault}()`);
 let gate:Promise<unknown>|undefined,writer:ReturnType<typeof selectWorkRole>|undefined,cleanup:Promise<{id:string}>|undefined;
 try{
  const observed=await measureReads(async()=>{
   gate=db.transaction(async tx=>{await tx.prepare('SELECT pg_advisory_xact_lock(hashtextextended(?,0))').bind(fault).run();ready();await gateRelease;});
   await Promise.race([gateReady,gate.then(()=>{throw Error('Native selector barrier ended before ready');})]);
   writer=selectWorkRole(db,{sessionId:authority.sessionId,userId:reviewer.userId,companyId:company,assignmentId:reviewer.assignmentId,assignmentVersion:authority.assignmentVersion,revision:original.revision,expiresAt:original.expires_at,rememberPreference:false});const writerResult=Promise.allSettled([writer]);
   let entered=false;for(let attempt=0;attempt<100;attempt++){entered=!!await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called');if(entered)break;await new Promise(resolve=>setTimeout(resolve,10));}expect(entered).toBe(true);
   cleanup=cancelCommentUploadDraft(db,worker,f.comment.id,command);const cleanupResult=Promise.allSettled([cleanup]);let blocked:number|null=null;
   try{
    for(let attempt=0;attempt<100;attempt++){blocked=await db.prepare("SELECT a.pid FROM pg_stat_activity a JOIN pg_locks l ON l.pid=a.pid WHERE a.datname=current_database() AND a.query='LOCK TABLE active_work_contexts IN EXCLUSIVE MODE' AND a.wait_event_type='Lock' AND l.relation='active_work_contexts'::regclass AND l.mode='ExclusiveLock' AND NOT l.granted").first<number>('pid');if(blocked!==null)break;await new Promise(resolve=>setTimeout(resolve,10));}expect(blocked).not.toBeNull();
    expect(await db.prepare("SELECT 1 FROM pg_locks WHERE pid=? AND granted AND relation=ANY(ARRAY['command_receipts'::regclass,'social_post_drafts'::regclass,'attachments'::regclass,'audit_events'::regclass])").bind(blocked).first()).toBeNull();
    for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();
   }finally{release();await gate;}
   const [selected,completed]=await Promise.all([writerResult,cleanupResult]);if(selected[0].status!=='fulfilled')throw selected[0].reason;if(completed[0].status!=='fulfilled')throw completed[0].reason;return {selected:selected[0].value,completed:completed[0].value};
  });
  expect({deadlockFailures:observed.metrics.deadlockFailures??0,serializationFailures:observed.metrics.serializationFailures??0,transactionRetries:observed.metrics.transactionRetries??0}).toEqual({deadlockFailures:0,serializationFailures:0,transactionRetries:0});expect(observed.result.selected).toMatchObject({assignment_id:reviewer.assignmentId,assignment_version:authority.assignmentVersion,revision:original.revision+1});expect(observed.result.completed).toEqual({id:f.comment.id});
  const after=await projectCleanupState([f.comment.id],command);expect(await cancelCommentUploadDraft(db,worker,f.comment.id,command)).toEqual({id:f.comment.id});expect(await projectCleanupState([f.comment.id],command)).toBe(after);expect(await db.prepare('SELECT to_jsonb(p)::text image FROM social_posts p WHERE id=?').bind(f.published.id).first('image')).toBe(parent);expect(await db.prepare('SELECT revoked_at FROM attachments WHERE id=?').bind(f.commentFile.sourceId).first('revoked_at')).not.toBeNull();expect(await db.prepare('SELECT status FROM upload_intents WHERE id=?').bind(f.commentFile.uploadIntentId).first('status')).toBe('expired');
 }finally{release();await Promise.allSettled([...(gate?[gate]:[]),...(writer?[writer]:[]),...(cleanup?[cleanup]:[])]);await db.exec(`DROP TRIGGER ${fault} ON active_work_contexts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
});

it('projectUploadCleanup rejects immediate born-receipt result and request-hash drift atomically before exact key retry',async()=>{
 const f=await projectCleanupFixture('enforced');await f.removeMember();const {cancelCommentUploadDraft,cancelPostEditUploadDraft}=await import('../../../src/server/modules/collaboration/public/social-home');
 for(const kind of ['result','request'] as const){
  const command=key(),fault=`social_receipt_${key().replaceAll('-','')}`,draft=kind==='result'?f.comment:f.edit,cancel=kind==='result'?cancelCommentUploadDraft:cancelPostEditUploadDraft,before=await projectCleanupState([f.comment.id,f.edit.id],command),mutation=kind==='result'?`UPDATE command_receipts SET result_json=jsonb_build_object('id','foreign-unrelated-draft')::text WHERE command_id=NEW.command_id;`:`UPDATE command_receipts SET request_hash=repeat('0',64) WHERE command_id=NEW.command_id;`;
  await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');${mutation}RETURN NEW;END $$;CREATE TRIGGER ${fault} AFTER INSERT ON command_receipts FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
  try{await expect(cancel(db,worker,draft.id,command)).rejects.toMatchObject({code:kind==='result'?'SOCIAL_UPLOAD_CLEANUP_CHANGED':'IDEMPOTENCY_CONFLICT'});expect(await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called')).toBe(true);expect(await projectCleanupState([f.comment.id,f.edit.id],command)).toBe(before);}
  finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
  expect(await cancel(db,worker,draft.id,command)).toEqual({id:draft.id});const after=await projectCleanupState([f.comment.id,f.edit.id],command);expect(await cancel(db,worker,draft.id,command)).toEqual({id:draft.id});expect(await projectCleanupState([f.comment.id,f.edit.id],command)).toBe(after);
 }
});
