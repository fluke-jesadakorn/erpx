import { readListPage } from "./list-page";
import type { WorkContext } from "./authorization";
import { hashtags, type Mention } from "@/lib/social-text";
import { plainRichContent, validateRichContent, type RichContent, type RichNode } from "@/lib/home-rich-content";
import { mentionsSchema, validateMentions, textStatements } from "./social-text";
import type { SqlDatabase } from "./runtime/types";
import { z } from "zod";
import { ApiError } from "./http";
import { assertion, executeCommand, replayCommand } from "./commands";
import { getProject, getTask } from "./staff-tasks";
import { readablePmsSource, pmsSourceLink } from "./pms/source";
import { pmsSourcesSql, pmsSourceReadableSql } from "./pms/source-policy";
import {
  readablePost,
  sourceAccessSql,
  sourceGuard,
  type PostRow,
} from "./social-source";
import {
  notification,
  authorityGuard,
  requireCompanyAuthority,
  type StaffActor,
} from "./staff-policy";
type Actor = WorkContext;
const postContentBlockSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("text"), text: z.string().max(30000) }),
  z.object({
    type: z.literal("image"),
    slotId: z.string().min(1).max(160),
    attachmentId: z.string().min(1).max(160).nullable(),
    name: z.string().min(1).max(255),
    alt: z.string().max(500).optional(),
    width: z.number().int().min(25).max(100).optional(),
  }),
]);
export const postContentSchema = z.array(postContentBlockSchema).max(100);
export type PostContent = z.infer<typeof postContentSchema>;
export const postPurposeSchema = z.enum(["update", "plan", "blocker", "handover", "announcement"]);
export type PostPurpose = z.infer<typeof postPurposeSchema>;
export const postAudienceSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("company") }),
  z.object({ kind: z.literal("branch"), id: z.string().min(1).max(160) }),
  z.object({ kind: z.literal("department"), id: z.string().min(1).max(160) }),
  z.object({ kind: z.literal("participants"), userIds: z.array(z.string().min(1).max(160)).min(1).max(20) }),
  z.object({ kind: z.literal("team"), id: z.string().min(1).max(160) }),
  z.object({ kind: z.literal("project"), id: z.string().min(1).max(160) }),
]);
export type PostAudience = z.infer<typeof postAudienceSchema>;
export const postSourceSchema = z.object({
  kind: z.enum(["staff_task", "staff_project", "pms_record"]),
  id: z.string().min(1).max(160),
  version: z.number().int().positive().optional(),
});
export type PostSource = z.infer<typeof postSourceSchema>;
function pmsSourceAssertions(db: SqlDatabase, key: string, actor: Actor, audience: PostAudience | null, sources: PostSource[]) {
  return sources.filter(source => source.kind === "pms_record").flatMap((source, index) => {
    const users = [...new Set([actor.userId, ...(audience?.kind === "participants" ? audience.userIds : [])])];
    return users.map((user, recipient) => assertion(db,key,`pms-source-${index}-${recipient}`,
      `EXISTS(SELECT 1 FROM (${pmsSourcesSql}) pr CROSS JOIN (SELECT ?::text user_id) recipient
        WHERE pr.id=? AND pr.company_id=? AND (?::bigint IS NULL OR pr.version=?) AND ${pmsSourceReadableSql("pr","recipient.user_id")})`,
      [user, source.id, actor.companyId, source.version ?? null, source.version ?? null]));
  });
}
export async function socialAudienceOptions(db: SqlDatabase, actor: Actor) {
  const [branches, teams, projects] = await Promise.all([
    db.prepare("SELECT id,display_name FROM organization_nodes WHERE kind='branch' AND company_id=? AND status='active' AND (?::text IS NULL OR id=?)")
      .bind(actor.companyId, actor.branchId, actor.branchId).all<{ id: string; display_name: string }>(),
    db.prepare(`SELECT t.id,t.name FROM messaging_teams t WHERE t.company_id=? AND NOT t.organization_managed
      AND EXISTS(SELECT 1 FROM messaging_team_members m WHERE m.team_id=t.id AND m.user_id=?) ORDER BY t.name,t.id LIMIT 100`)
      .bind(actor.companyId, actor.userId).all<{ id: string; name: string }>(),
    db.prepare(`SELECT p.id,p.title FROM staff_projects p WHERE p.company_id=?
      AND (p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?))
      ORDER BY p.title,p.id LIMIT 100`)
      .bind(actor.companyId, actor.userId, actor.userId).all<{ id: string; title: string }>(),
  ]);
  const base = actor.assignmentScope === "holding"
    ? [{ value: "company", kind: "company" as const, label: "Everyone in the company" },
      ...branches.results.map(b => ({ value: `branch:${b.id}`, kind: "branch" as const, label: `Branch · ${b.display_name}` }))]
    : actor.assignmentScope === "department" && actor.departmentId
      ? [{ value: `department:${actor.departmentId}`, kind: "department" as const,
          label: `Department · ${actor.scope.kind === "department" ? actor.scope.departmentName : "Your department"}` }]
      : branches.results.filter(b => b.id === actor.branchId).map(b =>
          ({ value: `branch:${b.id}`, kind: "branch" as const, label: `Branch · ${b.display_name}` }));
  const within = actor.assignmentScope === "holding" ? "" : actor.assignmentScope === "department" ? " · in your department" : " · in your branch";
  return { branches: branches.results, projects: projects.results,
    audiences: [...base,
      { value: "participants", kind: "participants" as const, label: `Selected people${within}` },
      ...teams.results.map(t => ({ value: `team:${t.id}`, kind: "team" as const, label: `Team · ${t.name}${within}` })),
      ...projects.results.map(p => ({ value: `project:${p.id}`, kind: "project" as const, label: `Project · ${p.title}${within}` }))] };
}
function contentBody(content: PostContent) {
  const text = content.filter((block) => block.type === "text").map((block) => block.text);
  return text.some((value) => value.trim()) ? text.join("\n") : "";
}
function contentImages(content: PostContent) {
  return content.filter((block) => block.type === "image");
}
function storedContent(json: string | null | undefined): PostContent | null {
  return json ? postContentSchema.parse(JSON.parse(json)) : null;
}
export const postDraftSchema = z.object({
  body: z.string().max(30000),
  content: postContentSchema.optional(),
  richContent: z.unknown().optional(),
  mentions: mentionsSchema.optional(),
  audience: postAudienceSchema.nullable().optional(),
  sources: z.array(postSourceSchema).max(5).optional(),
  // Existing API clients supplied branchId. A supplied value still denotes an
  // explicit branch/company choice; new composers send audience instead.
  branchId: z.string().max(160).nullable().optional(),
  purpose: postPurposeSchema.optional(),
  expectedVersion: z.number().int().positive().default(1),
});
export type PostDraft = {
  id: string;
  author_id: string;
  company_id: string;
  branch_id: string | null;
  audience_kind: PostAudience["kind"] | null;
  audience_id: string | null;
  participants_json: string;
  sources_json: string;
  purpose: PostPurpose;
  body: string;
  department_id: string | null;
  version: number;
  published_post_id: string | null;
  mentions_json: string;
  content_json: string | null;
  rich_content_json: string | null;
  comment_parent_post_id: string|null;
  published_comment_id: string|null;
  cancelled_at: number|null;
  edit_post_id: string|null;
  edit_post_version: number|null;
  edit_saved_version: number|null;
};
function selectedAudience(actor: Actor, input: z.infer<typeof postDraftSchema>): PostAudience | null {
  if (input.audience !== undefined) return input.audience;
  if (input.branchId === undefined) return null;
  if (input.branchId === null) return { kind: "company" };
  if (actor.departmentId) {
    if (input.branchId !== actor.branchId)
      throw new ApiError(403, "AUDIENCE_FORBIDDEN", "Your current role cannot share with this branch.");
    return { kind: "department", id: actor.departmentId };
  }
  return { kind: "branch", id: input.branchId };
}
function audienceScope(actor: Actor, audience: PostAudience | null) {
  if (!audience) return { branch: null, department: null, audienceId: null, participants: [] as string[] };
  if (audience.kind === "company") {
    if (actor.assignmentScope !== "holding")
      throw new ApiError(403, "AUDIENCE_FORBIDDEN", "Your current role cannot share with the whole company.");
    return { branch: null, department: null, audienceId: null, participants: [] as string[] };
  }
  if (audience.kind === "branch") {
    if (actor.assignmentScope !== "holding" &&
        !(actor.assignmentScope === "branch" && actor.branchId === audience.id))
      throw new ApiError(403, "AUDIENCE_FORBIDDEN", "Your current role cannot share with this branch.");
    return { branch: audience.id, department: null, audienceId: null, participants: [] as string[] };
  }
  if (audience.kind === "department") {
    if (actor.assignmentScope !== "department" || actor.departmentId !== audience.id)
      throw new ApiError(403, "AUDIENCE_FORBIDDEN", "Your current role cannot share with this department.");
    return { branch: actor.branchId, department: audience.id, audienceId: null, participants: [] as string[] };
  }
  const participants = audience.kind === "participants"
    ? [...new Set(audience.userIds.filter(id => id !== actor.userId))].sort() : [];
  if (audience.kind === "participants" && !participants.length)
    throw new ApiError(422, "AUDIENCE_REQUIRED", "Select at least one other person.");
  return { branch: actor.assignmentScope === "holding" ? null : actor.branchId,
    department: actor.assignmentScope === "department" ? actor.departmentId : null,
    audienceId: audience.kind === "team" || audience.kind === "project" ? audience.id : null,
    participants };
}
function audienceAssertion(db: SqlDatabase, key: string, actor: Actor, audience: PostAudience | null) {
  if (!audience || audience.kind === "company") return [];
  if (audience.kind === "branch" || audience.kind === "department")
    return [assertion(db, key, "audience", "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind=? AND status='active')",
      [audience.id, actor.companyId, audience.kind])];
  if (audience.kind === "team") return [assertion(db, key, "audience-team",
    "EXISTS(SELECT 1 FROM messaging_teams t JOIN messaging_team_members m ON m.team_id=t.id WHERE t.id=? AND t.company_id=? AND NOT t.organization_managed AND m.user_id=?)",
    [audience.id, actor.companyId, actor.userId])];
  if (audience.kind === "project") return [assertion(db, key, "audience-project",
    "EXISTS(SELECT 1 FROM staff_projects p WHERE p.id=? AND p.company_id=? AND (p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?)))",
    [audience.id, actor.companyId, actor.userId, actor.userId])];
  return [...new Set([actor.userId, ...audience.userIds])].map((userId, index) => assertion(db, key, `audience-person-${index}`,
    `EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=? AND a.company_id=? AND a.revoked_at IS NULL
      AND a.valid_from<=? AND a.valid_until>? AND (CASE
        WHEN ?::text IS NOT NULL THEN a.scope_kind='department' AND a.scope_node_id=?
        WHEN ?::text IS NOT NULL THEN a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?)
          OR (a.scope_kind='department' AND EXISTS(SELECT 1 FROM organization_nodes n
            WHERE n.id=a.scope_node_id AND n.company_id=a.company_id AND n.branch_id=?))
        ELSE TRUE END))`,
    [userId, actor.companyId, Date.now(), Date.now(), actor.departmentId, actor.departmentId,
      actor.branchId, actor.branchId, actor.branchId]));
}
async function validatePostSources(db: SqlDatabase, actor: Actor, sources: PostSource[]) {
  if (new Set(sources.map(s => `${s.kind}:${s.id}`)).size !== sources.length)
    throw new ApiError(422, "SOURCE_DUPLICATE", "Choose each linked record once.");
  for (const source of sources) {
    if (source.kind === "staff_task") await getTask(db, actor, source.id);
    else if (source.kind === "pms_record") {
      const item = await readablePmsSource(db, actor, source.id);
      if (source.version && Number(item.version) !== source.version)
        throw new ApiError(409,"HANDOVER_SOURCE_CHANGED","Hotel work changed. Refresh before publishing.");
    }
    else await getProject(db, actor, source.id);
  }
}
export async function socialUploadOwner(
  db: SqlDatabase,
  actor: StaffActor,
  id: string,
  edit = false,
) {
  const draft = await db
    .prepare("SELECT * FROM social_post_drafts WHERE id=? AND company_id=?")
    .bind(id, actor.companyId)
    .first<PostDraft>();
  if (!draft) throw new ApiError(404, "DRAFT_NOT_FOUND", "Draft unavailable.");
  if(draft.cancelled_at)throw new ApiError(404,"DRAFT_NOT_FOUND","Upload draft unavailable.");
  if (draft.edit_post_id) {
    const post = await readablePost(db, actor, draft.edit_post_id);
    if (edit && (draft.author_id !== actor.userId || draft.edit_saved_version !== null ||
      post.author_id !== actor.userId || post.kind !== "team" || post.linked_module))
      throw new ApiError(403, "POST_AUTHOR_REQUIRED", "Only the author can attach images to this post edit.");
    if (edit && post.version !== draft.edit_post_version)
      throw new ApiError(409, "POST_CHANGED", "This post changed. Reload it before attaching images.");
    return draft;
  }
  if(draft.comment_parent_post_id){
    if(edit){if(draft.author_id!==actor.userId||draft.published_comment_id)throw new ApiError(403,"DRAFT_LOCKED","This comment no longer accepts images.");await readablePost(db,actor,draft.comment_parent_post_id);}
    else if(draft.published_comment_id){await readablePost(db,actor,draft.comment_parent_post_id);if(!await db.prepare("SELECT 1 FROM social_comments WHERE id=? AND post_id=? AND state='published'").bind(draft.published_comment_id,draft.comment_parent_post_id).first())throw new ApiError(404,"COMMENT_UNAVAILABLE","This image is no longer available.");}
    else if(draft.author_id!==actor.userId)throw new ApiError(404,"DRAFT_NOT_FOUND","Upload draft unavailable.");
    return draft;
  }
  if (edit || !draft.published_post_id) {
    if (draft.author_id !== actor.userId || draft.published_post_id)
      throw new ApiError(
        403,
        "DRAFT_LOCKED",
        "Only the author can edit an unpublished draft.",
      );
  } else await readablePost(db, actor, draft.published_post_id);
  return draft;
}
export async function createCommentUploadDraft(db:SqlDatabase,actor:StaffActor,postId:string,key:string){
  const post=await readablePost(db,actor,postId),id=`comment-upload-${key}`,now=Date.now();
  return executeCommand(db,actor,key,"social.comment.upload.create",{postId},id,1,{id},[
    sourceGuard(db,key,post,actor.userId,"source"),
    db.prepare("INSERT INTO social_post_drafts(id,author_id,company_id,branch_id,body,created_at,updated_at,comment_parent_post_id) VALUES(?,?,?,?, '',?,?,?)")
      .bind(id,actor.userId,actor.companyId,post.branch_id,now,now,postId),
  ]);
}
export async function cancelCommentUploadDraft(db:SqlDatabase,actor:StaffActor,id:string,key:string){
  const draft=await socialUploadOwner(db,actor,id,true);
  if(!draft.comment_parent_post_id)throw new ApiError(404,"DRAFT_NOT_FOUND","Comment upload draft unavailable.");
  const now=Date.now();
  return executeCommand(db,actor,key,"social.comment.upload.cancel",{id},id,draft.version+1,{id},[
    assertion(db,key,"draft","EXISTS(SELECT 1 FROM social_post_drafts WHERE id=? AND author_id=? AND published_comment_id IS NULL AND cancelled_at IS NULL)",[id,actor.userId]),
    db.prepare("UPDATE social_post_drafts SET cancelled_at=?,version=version+1,updated_at=? WHERE id=?").bind(now,now,id),
    db.prepare("UPDATE attachments SET revoked_at=? WHERE owner_type='social_draft' AND owner_id=? AND revoked_at IS NULL").bind(now,id),
    db.prepare("UPDATE upload_intents SET status='expired',updated_at=? WHERE owner_type='social_draft' AND owner_id=? AND status='ready'").bind(now,id),
  ]);
}
export async function createPostEditUploadDraft(db:SqlDatabase,actor:StaffActor,postId:string,expectedVersion:number,key:string){
  const post=await readablePost(db,actor,postId);
  if(post.author_id!==actor.userId||post.kind!=="team"||post.linked_module)
    throw new ApiError(403,"POST_AUTHOR_REQUIRED","Only the author can edit this team post.");
  if(post.version!==expectedVersion)
    throw new ApiError(409,"POST_CHANGED","This post changed. Reload it before editing.");
  const id=`post-edit-upload-${key}`,now=Date.now();
  return executeCommand(db,actor,key,"social.post.edit-upload.create",{postId,expectedVersion},id,1,{id,version:1},[
    sourceGuard(db,key,post,actor.userId,"source"),
    db.prepare("INSERT INTO social_post_drafts(id,author_id,company_id,branch_id,body,created_at,updated_at,edit_post_id,edit_post_version) VALUES(?,?,?,?, '',?,?,?,?)")
      .bind(id,actor.userId,actor.companyId,post.branch_id,now,now,postId,expectedVersion),
  ]);
}
export async function cancelPostEditUploadDraft(db:SqlDatabase,actor:StaffActor,id:string,key:string){
  const draft=await db.prepare("SELECT * FROM social_post_drafts WHERE id=? AND company_id=?")
    .bind(id,actor.companyId).first<PostDraft>();
  if(!draft||!draft.edit_post_id||draft.edit_saved_version!==null||draft.cancelled_at!==null)
    throw new ApiError(404,"UPLOAD_DRAFT_UNAVAILABLE","Post edit upload draft unavailable.");
  await readablePost(db,actor,draft.edit_post_id);
  if(draft.author_id!==actor.userId)
    throw new ApiError(404,"UPLOAD_DRAFT_UNAVAILABLE","Post edit upload draft unavailable.");
  const now=Date.now();
  return executeCommand(db,actor,key,"social.post.edit-upload.cancel",{id},id,draft.version+1,{id},[
    assertion(db,key,"draft","EXISTS(SELECT 1 FROM social_post_drafts WHERE id=? AND author_id=? AND edit_post_id IS NOT NULL AND edit_saved_version IS NULL AND cancelled_at IS NULL)",[id,actor.userId]),
    db.prepare("UPDATE social_post_drafts SET cancelled_at=?,version=version+1,updated_at=? WHERE id=?").bind(now,now,id),
    db.prepare("UPDATE attachments SET revoked_at=? WHERE owner_type='social_draft' AND owner_id=? AND revoked_at IS NULL").bind(now,id),
    db.prepare("UPDATE upload_intents SET status='expired',updated_at=? WHERE owner_type='social_draft' AND owner_id=? AND status='ready'").bind(now,id),
  ]);
}
export async function savePostDraft(
  db: SqlDatabase,
  actor: Actor,
  id: string | null,
  input: z.infer<typeof postDraftSchema>,
  key: string,
) {
  const recordId = id ?? `post-draft-${key}`,
    now = Date.now();
  const existing = id ? await socialUploadOwner(db, actor, id, true) : null;
  if (existing?.published_post_id || existing?.edit_post_id || id&&await db.prepare("SELECT 1 FROM social_post_drafts WHERE id=? AND comment_parent_post_id IS NOT NULL").bind(id).first())
    throw new ApiError(404,"DRAFT_NOT_FOUND","This is a comment upload draft.");
  if(id&&input.richContent===undefined&&await db.prepare("SELECT 1 FROM social_post_drafts WHERE id=? AND rich_content_json IS NOT NULL").bind(id).first())
    throw new ApiError(422,"RICH_CONTENT_REQUIRED","Reload this draft in the advanced editor before saving changes.");
  const content = input.content;
  let rich:RichContent|null=null;
  if(input.richContent!==undefined) {
    try {const parsed=validateRichContent(input.richContent,30000);rich=parsed.content;
      if(parsed.body!==input.body||JSON.stringify(parsed.mentions)!==JSON.stringify(input.mentions??[]))throw Error("Rich text and mentions differ from the message.");
      if(content&&rich.blocks.length!==content.length)throw Error("Rich text blocks differ from post content.");
      if(content&&rich.blocks.some((block,i)=>block.type!==content[i].type||(block.type==="image"&&content[i].type==="image"&&(block.slotId!==content[i].slotId||block.attachmentId!==content[i].attachmentId||block.width!==content[i].width))))throw Error("Rich images differ from post content.");
    }catch(error){throw new ApiError(422,"RICH_CONTENT_INVALID",error instanceof Error?error.message:"Invalid rich content.");}
  }
  if (content) {
    if (contentBody(content) !== input.body)
      throw new ApiError(422, "POST_CONTENT_CHANGED", "The post text changed. Review the draft and try again.");
    const images = contentImages(content);
    if (images.length > 10)
      throw new ApiError(422, "ATTACHMENT_LIMIT", "Posts accept up to ten images.");
    if (new Set(images.map((image) => image.slotId)).size !== images.length ||
        new Set(images.filter((image) => image.attachmentId).map((image) => image.attachmentId)).size !== images.filter((image) => image.attachmentId).length)
      throw new ApiError(422, "POST_IMAGE_DUPLICATE", "Each post image needs a unique slot and file.");
  }
  const mentions = await validateMentions(db, actor, input.body, input.mentions);
  const keptAttachmentIds = content ? contentImages(content).flatMap((image) => image.attachmentId ? [image.attachmentId] : []) : [];
  const audience = selectedAudience(actor, input);
  const { branch, department, audienceId, participants } = audienceScope(actor, audience);
  const sources = input.sources ?? [];
  await validatePostSources(db, actor, sources);
  const purpose = input.purpose ?? "update";
  return executeCommand(
    db,
    actor,
    key,
    "social.draft.save",
    input,
    recordId,
    id ? input.expectedVersion + 1 : 1,
    { id: recordId, version: id ? input.expectedVersion + 1 : 1 },
    [
      ...audienceAssertion(db, key, actor, audience),
      ...pmsSourceAssertions(db, key, actor, audience, sources),
      ...(id
        ? [
            assertion(
              db,
              key,
              "draft-version",
              "EXISTS(SELECT 1 FROM social_post_drafts WHERE id=? AND author_id=? AND version=? AND published_post_id IS NULL)",
              [id, actor.userId, input.expectedVersion],
            ),
          ]
        : []),
      id
        ? db
            .prepare(
              "UPDATE social_post_drafts SET body=?,branch_id=?,department_id=?,audience_kind=?,audience_id=?,participants_json=?,sources_json=?,purpose=?,mentions_json=?,content_json=?,rich_content_json=?,version=version+1,updated_at=? WHERE id=?",
            )
            .bind(input.body, branch, department, audience?.kind ?? null, audienceId, JSON.stringify(participants), JSON.stringify(sources), purpose, JSON.stringify(mentions), content ? JSON.stringify(content) : null, rich?JSON.stringify(rich):null, now, id)
        : db
            .prepare(
              "INSERT INTO social_post_drafts(id,author_id,company_id,branch_id,body,created_at,updated_at,department_id,audience_kind,audience_id,participants_json,sources_json,purpose,mentions_json,content_json,rich_content_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            )
            .bind(
              recordId,
              actor.userId,
              actor.companyId,
              branch,
              input.body,
              now,
              now,
              department,
              audience?.kind ?? null,
              audienceId,
              JSON.stringify(participants),
              JSON.stringify(sources),
              purpose,
              JSON.stringify(mentions),
              content ? JSON.stringify(content) : null,
              rich?JSON.stringify(rich):null,
            ),
      ...(content ? [
        db.prepare(`UPDATE attachments SET revoked_at=? WHERE owner_type='social_draft' AND owner_id=? AND revoked_at IS NULL${keptAttachmentIds.length ? ` AND id NOT IN (${keptAttachmentIds.map(() => "?").join(",")})` : ""}`)
          .bind(now, recordId, ...keptAttachmentIds),
        db.prepare("UPDATE upload_intents u SET status='expired',updated_at=? WHERE u.owner_type='social_draft' AND u.owner_id=? AND u.status='ready' AND NOT EXISTS(SELECT 1 FROM attachments a WHERE a.upload_id=u.id AND a.revoked_at IS NULL)")
          .bind(now, recordId),
      ] : []),
    ],
  );
}
export async function publishPost(
  db: SqlDatabase,
  actor: Actor,
  id: string,
  version: number,
  key: string,
) {
  const draft = await db
    .prepare(
      "SELECT * FROM social_post_drafts WHERE id=? AND company_id=? AND author_id=?",
    )
    .bind(id, actor.companyId, actor.userId)
    .first<PostDraft>();
  if (!draft) throw new ApiError(404, "DRAFT_NOT_FOUND", "Draft unavailable.");
  if(draft.comment_parent_post_id)throw new ApiError(404,"DRAFT_NOT_FOUND","This is a comment upload draft.");
  if(draft.edit_post_id)throw new ApiError(404,"DRAFT_NOT_FOUND","This is a post edit upload draft.");
  if (draft.published_post_id) {
    await readablePost(db, actor, draft.published_post_id);
    return { id: draft.published_post_id, version: 1 };
  }
  if (!draft.audience_kind)
    throw new ApiError(422, "AUDIENCE_REQUIRED", "Choose who can see this post before publishing.");
  const audience: PostAudience = draft.audience_kind === "company"
    ? { kind: "company" }
    : draft.audience_kind === "participants"
      ? { kind: "participants", userIds: JSON.parse(draft.participants_json) as string[] }
      : { kind: draft.audience_kind, id: draft.audience_kind === "branch" ? draft.branch_id!
        : draft.audience_kind === "department" ? draft.department_id! : draft.audience_id! };
  audienceScope(actor, audience);
  const sources = z.array(postSourceSchema).max(5).parse(JSON.parse(draft.sources_json));
  await validatePostSources(db, actor, sources);
  const content = storedContent(draft.content_json);
  const images = content ? contentImages(content) : [];
  if (!draft.body.trim() && !images.length)
    throw new ApiError(
      422,
      "POST_BODY_REQUIRED",
      "Add a message before publishing.",
    );
  if (content && contentBody(content) !== draft.body)
    throw new ApiError(422, "POST_CONTENT_CHANGED", "The post text changed. Review the draft and try again.");
  if (images.length > 10)
    throw new ApiError(422, "ATTACHMENT_LIMIT", "Posts accept up to ten images.");
  if (images.some((image) => !image.attachmentId))
    throw new ApiError(422, "POST_IMAGE_PENDING", "Finish uploading every image before publishing.");
  if (images.length) {
    const ids = images.map((image) => image.attachmentId!);
    const found = (await db.prepare(`SELECT a.id FROM attachments a JOIN upload_intents u ON u.id=a.upload_id JOIN social_post_drafts d ON d.id=a.owner_id
      WHERE a.id IN (${ids.map(() => "?").join(",")}) AND a.owner_type='social_draft' AND a.owner_id=? AND d.company_id=? AND a.revoked_at IS NULL
      AND a.media_type IN ('image/jpeg','image/png','image/webp') AND u.owner_type='social_draft' AND u.owner_id=d.id AND u.status='ready'`)
      .bind(...ids, id, actor.companyId).all<{id: string}>()).results;
    if (found.length !== ids.length)
      throw new ApiError(422, "POST_IMAGE_UNAVAILABLE", "An image is unavailable. Reattach it before publishing.");
  }
  const mentions = await validateMentions(db, actor, draft.body, JSON.parse(draft.mentions_json));
  const postId = `post-${id}`,
    now = Date.now();
  return executeCommand(
    db,
    actor,
    key,
    "social.post.publish",
    { id, version },
    postId,
    1,
    { id: postId, version: 1 },
    [
      assertion(
        db,
        key,
        "draft",
        "EXISTS(SELECT 1 FROM social_post_drafts WHERE id=? AND author_id=? AND version=? AND published_post_id IS NULL)",
        [id, actor.userId, version],
      ),
      ...audienceAssertion(db, key, actor, audience),
      ...pmsSourceAssertions(db, key, actor, audience, sources),
      ...(content ? images.map((image, index) => assertion(
        db,
        key,
        `image-${index}`,
        `EXISTS(SELECT 1 FROM attachments a JOIN upload_intents u ON u.id=a.upload_id JOIN social_post_drafts d ON d.id=a.owner_id
          WHERE a.id=? AND a.owner_type='social_draft' AND a.owner_id=? AND d.company_id=? AND a.revoked_at IS NULL
          AND a.media_type IN ('image/jpeg','image/png','image/webp') AND u.owner_type='social_draft' AND u.owner_id=d.id AND u.status='ready')`,
        [image.attachmentId, id, actor.companyId],
      )) : [assertion(
        db,
        key,
        "uploads",
        "NOT EXISTS(SELECT 1 FROM upload_intents WHERE owner_type='social_draft' AND owner_id=? AND status IN ('pending','uploaded') AND expires_at>?)",
        [id, now],
      )]),
      db
        .prepare(
          "INSERT INTO social_posts(id,kind,author_id,author_name,company_id,branch_id,body,last_command_id,created_at,updated_at,department_id,audience_kind,audience_id,purpose,mentions_json,content_json,rich_content_json) VALUES(?,'team',?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          postId,
          actor.userId,
          actor.name,
          actor.companyId,
          draft.branch_id,
          draft.body,
          key,
          now,
          now,
          draft.department_id,
          draft.audience_kind,
          draft.audience_id,
          draft.purpose,
          JSON.stringify(mentions),
          draft.content_json,
          draft.rich_content_json,
        ),
      ...(audience.kind === "participants" ? [...new Set([actor.userId, ...audience.userIds])].map(userId =>
        db.prepare("INSERT INTO social_post_participants(post_id,user_id) VALUES(?,?)").bind(postId, userId)) : []),
      ...sources.map((source, ordinal) => db.prepare(
        "INSERT INTO social_post_sources(post_id,ordinal,source_kind,source_id) VALUES(?,?,?,?)")
        .bind(postId, ordinal, source.kind, source.id)),
      ...await textStatements(db, actor, postId, "post", postId, draft.body, mentions),
      db
        .prepare(
          "UPDATE social_post_drafts SET published_post_id=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(postId, now, id),
      db
        .prepare(
          "INSERT INTO social_follows(principal_id,target_type,target_id,created_at) VALUES(?,'post',?,?) ON CONFLICT DO NOTHING",
        )
        .bind(actor.userId, postId, now),
    ],
    { companyId: actor.companyId, branchId: draft.branch_id, departmentId: draft.department_id },
  );
}
type CommentRow = { id:string;post_id:string;body:string;author_id:string;author_name:string;created_at:number;mentions_json:string;rich_content_json:string|null;author_kind:"person"|"agent";parent_comment_id:string|null;parent_author_name:string|null };
const commentView = (c:CommentRow) => ({...c,mentions:JSON.parse(c.mentions_json) as Mention[],richContent:c.rich_content_json?JSON.parse(c.rich_content_json) as RichContent:null});
export async function postComments(db:SqlDatabase,actor:StaffActor,id:string,cursor?:string) {
  await readablePost(db,actor,id);
  const page = await readListPage<CommentRow & {page_key:string}>(db,{
    sql:"SELECT c.id,c.post_id,c.body,c.author_id,c.author_name,c.created_at,c.mentions_json,c.rich_content_json,c.author_kind,c.parent_comment_id,parent.author_name AS parent_author_name,c.created_at::text AS page_key FROM social_comments c LEFT JOIN social_comments parent ON parent.id=c.parent_comment_id AND parent.post_id=c.post_id WHERE c.post_id=? AND c.state='published'",
    args:[id],scope:[actor.companyId,actor.userId,id],cursor,limit:20,descending:true,
  });
  const ancestors = await commentAncestors(db,id,page.items);
  return {...page,items:[...page.items,...ancestors].map(commentView)};
}
async function commentAncestors(db:SqlDatabase,postId:string,seeds:CommentRow[]) {
  const ids=[...new Set(seeds.filter(comment=>comment.parent_comment_id).map(comment=>comment.id))];
  if(!ids.length)return [] as CommentRow[];
  return (await db.prepare(`WITH RECURSIVE ancestors AS (
      SELECT parent.* FROM social_comments child JOIN social_comments parent
        ON parent.id=child.parent_comment_id AND parent.post_id=child.post_id
      WHERE child.post_id=? AND child.id IN (${ids.map(()=>"?").join(",")}) AND child.state='published'
      UNION
      SELECT parent.* FROM ancestors child JOIN social_comments parent
        ON parent.id=child.parent_comment_id AND parent.post_id=child.post_id
      WHERE parent.state='published'
    )
    SELECT a.id,a.post_id,a.body,a.author_id,a.author_name,a.created_at,a.mentions_json,a.rich_content_json,
      a.author_kind,a.parent_comment_id,parent.author_name AS parent_author_name
    FROM ancestors a LEFT JOIN social_comments parent ON parent.id=a.parent_comment_id AND parent.post_id=a.post_id
    WHERE a.state='published'`)
    .bind(postId,...ids).all<CommentRow>()).results;
}
export async function postViews(db:SqlDatabase,actor:StaffActor,ids:string[],commentLimit:number) {
  if(!ids.length) return [];
  const now=Date.now(), slots=ids.map(()=>'?').join(',');
  const posts=(await db.prepare(`SELECT p.*,
    CASE WHEN p.audience_kind='team' THEN (SELECT t.name FROM messaging_teams t WHERE t.id=p.audience_id AND t.company_id=p.company_id)
      WHEN p.audience_kind='project' THEN (SELECT pr.title FROM staff_projects pr WHERE pr.id=p.audience_id AND pr.company_id=p.company_id)
      ELSE (SELECT n.display_name FROM organization_nodes n WHERE n.id=COALESCE(p.department_id,p.branch_id) AND n.company_id=p.company_id) END AS audience_name,
    (SELECT count(*) FROM social_post_participants pp WHERE pp.post_id=p.id AND pp.user_id<>p.author_id) AS audience_count
    FROM social_posts p WHERE p.id IN (${slots}) AND p.company_id=? AND p.state='published' AND ${sourceAccessSql("p","?::text")}`)
    .bind(...ids,actor.companyId,actor.userId,now,now,actor.assignmentId).all<PostRow & {audience_name:string|null;audience_count:number}>()).results;
  if(!posts.length) return [];
  const allowed=posts.map(p=>p.id), placeholders=allowed.map(()=>'?').join(',');
  const [comments,files,counts,origins,replies,sources,aiSettings]=await Promise.all([
    db.prepare(`WITH RECURSIVE ranked AS (
        SELECT c.*,row_number() OVER(PARTITION BY c.post_id ORDER BY c.created_at DESC,c.id DESC) ordinal
        FROM social_comments c WHERE c.post_id IN (${placeholders}) AND c.state='published'
      ), selected AS (
        SELECT * FROM ranked WHERE ordinal<=?
      ), ancestors AS (
        SELECT parent.* FROM selected child JOIN social_comments parent
          ON parent.id=child.parent_comment_id AND parent.post_id=child.post_id
        WHERE parent.state='published'
        UNION
        SELECT parent.* FROM ancestors child JOIN social_comments parent
          ON parent.id=child.parent_comment_id AND parent.post_id=child.post_id
        WHERE parent.state='published'
      ), thread_comments AS (
        SELECT id,post_id,body,author_id,author_name,created_at,mentions_json,rich_content_json,author_kind,parent_comment_id FROM selected
        UNION
        SELECT id,post_id,body,author_id,author_name,created_at,mentions_json,rich_content_json,author_kind,parent_comment_id FROM ancestors
      )
      SELECT c.id,c.post_id,c.body,c.author_id,c.author_name,c.created_at,c.mentions_json,c.rich_content_json,c.author_kind,c.parent_comment_id,
        parent.author_name AS parent_author_name FROM thread_comments c
      LEFT JOIN social_comments parent ON parent.id=c.parent_comment_id AND parent.post_id=c.post_id
      ORDER BY c.created_at,c.id`)
      .bind(...allowed,commentLimit).all<CommentRow>(),
    db.prepare(`SELECT COALESCE(d.published_post_id,d.edit_post_id) AS post_id,a.id,a.original_name,a.media_type FROM attachments a JOIN social_post_drafts d ON d.id=a.owner_id
      WHERE a.owner_type='social_draft' AND (d.published_post_id IN (${placeholders}) OR (d.edit_post_id IN (${placeholders}) AND d.edit_saved_version IS NOT NULL))
        AND a.revoked_at IS NULL ORDER BY a.created_at,a.id`).bind(...allowed,...allowed)
      .all<{post_id:string;id:string;original_name:string;media_type:string}>(),
    db.prepare(`SELECT p.id,(SELECT count(*) FROM social_comments WHERE post_id=p.id AND state='published') AS commentCount,
      (SELECT count(*) FROM social_reactions WHERE post_id=p.id) AS appreciationCount,
      EXISTS(SELECT 1 FROM social_reactions WHERE post_id=p.id AND principal_id=?) AS appreciated,
      EXISTS(SELECT 1 FROM social_follows WHERE target_type='post' AND target_id=p.id AND principal_id=?) AS followed
      FROM social_posts p WHERE p.id IN (${placeholders})`).bind(actor.userId,actor.userId,...allowed)
      .all<{id:string;commentCount:number;appreciationCount:number;appreciated:number;followed:number}>(),
    db.prepare(`SELECT post_id,task_id FROM social_task_origins WHERE post_id IN (${placeholders})`).bind(...allowed).all<{post_id:string;task_id:string}>(),
    db.prepare(`SELECT post_id,id,owner_type,owner_id,state,error,error_code,actor_id,requester_name,attempt,('reply-' || id) AS reply_id FROM social_agent_replies WHERE post_id IN (${placeholders}) ORDER BY created_at`).bind(...allowed)
      .all<{post_id:string;id:string;owner_type:"post"|"comment";owner_id:string;state:string;error:string|null;error_code:string|null;actor_id:string;requester_name:string|null;attempt:number;reply_id:string}>(),
    db.prepare(`SELECT post_id,source_kind,source_id,ordinal FROM social_post_sources WHERE post_id IN (${placeholders}) ORDER BY post_id,ordinal`)
      .bind(...allowed).all<{post_id:string;source_kind:PostSource["kind"];source_id:string;ordinal:number}>(),
    db.prepare("SELECT home_ai_name FROM ai_company_settings WHERE company_id=?").bind(actor.companyId).first<{home_ai_name:string}>(),
  ]);
  const byId=new Map(posts.map(p=>[p.id,p]));
  return Promise.all(ids.filter(id=>byId.has(id)).map(async id=>{
    const post=byId.get(id)!,origin=origins.results.find(o=>o.post_id===id);
    let task:null|{id:string;state:string}=null,trackingState:string|null=null;
    if(origin){
      const {getTask}=await import('./staff-tasks');
      try {const t=await getTask(db,actor,origin.task_id);task={id:t.id,state:t.state};}
      catch(e){if(!(e instanceof ApiError)||![403,404].includes(e.status))throw e;}
      if(post.author_id===actor.userId) trackingState=await db.prepare('SELECT state FROM staff_tasks WHERE id=? AND company_id=?').bind(origin.task_id,actor.companyId).first<string>('state');
    }
    const count=counts.results.find(c=>c.id===id)!;
    const content=storedContent(post.content_json);
    const referencedImages=content ? new Set(contentImages(content).map(image=>image.attachmentId)) : null;
    const within = post.department_id ? " · in department" : post.branch_id ? " · in branch" : "";
    const audienceLabel = post.audience_kind === "participants" ? `Selected people (${post.audience_count})${within}`
      : post.audience_kind === "team" ? `Team · ${post.audience_name ?? "Team"}${within}`
      : post.audience_kind === "project" ? `Project · ${post.audience_name ?? "Project"}${within}`
      : post.department_id ? `Department · ${post.audience_name ?? "Your department"}`
      : post.branch_id ? `Branch · ${post.audience_name ?? "Branch"}` : "Everyone in the company";
    const linkedSources = await Promise.all(sources.results.filter(s=>s.post_id===id).map(async s => {
      try {
        if (s.source_kind === "pms_record") {
          const source = await readablePmsSource(db, actor, s.source_id);
          return { kind: s.source_kind, available: true as const, id: source.id, ...pmsSourceLink(source) };
        }
        if (s.source_kind === "staff_task") {
          const task = await getTask(db, actor, s.source_id);
          return { kind: s.source_kind, available: true as const, id: task.id, title: task.title,
            href: `/staff-tasks/${encodeURIComponent(task.id)}` };
        }
        const project = await getProject(db, actor, s.source_id);
        return { kind: s.source_kind, available: true as const, id: project.id, title: project.title,
          href: `/projects/${encodeURIComponent(project.id)}` };
      } catch (error) {
        if (!(error instanceof ApiError) || ![403, 404].includes(error.status)) throw error;
        return { kind: s.source_kind, available: false as const };
      }
    }));
    return {...post,audienceLabel,aiName:aiSettings?.home_ai_name??"AI",content,richContent:post.rich_content_json?JSON.parse(post.rich_content_json) as RichContent:null,mentions:JSON.parse(post.mentions_json??'[]') as Mention[],
      linkedSources,
      comments:comments.results.filter(c=>c.post_id===id).map(commentView),
      files:files.results.filter(f=>f.post_id===id && (!referencedImages || referencedImages.has(f.id))),agentReplies:replies.results.filter(r=>r.post_id===id),
      appreciationCount:count.appreciationCount,appreciated:count.appreciated,followed:count.followed,commentCount:count.commentCount,
      task,trackingState,hasTask:!!origin};
  }));
}
export async function postView(db:SqlDatabase,actor:StaffActor,id:string) {
  const post=(await postViews(db,actor,[id],20))[0];
  if(!post)throw new ApiError(404,'POST_NOT_FOUND','This post is unavailable.');
  return post;
}
export async function homePosts(
  db: SqlDatabase,
  actor: StaffActor,
  filter: {
    tag?: string;
    following?: boolean;
    branchId?: string;
    before?: number;
    beforeId?: string;
    limit?: number;
    propertyId?: string;
    purpose?: PostPurpose;
  } = {},
) {
  const now = Date.now();
  const limit = Math.max(1, Math.min(filter.limit ?? 20, 20));
  const result = await db
    .prepare(
      `SELECT p.id FROM social_posts p WHERE p.company_id=? AND p.state='published' AND p.linked_module IS NULL AND ${sourceAccessSql("p","?::text")} AND (?::text IS NULL OR p.branch_id=? OR p.branch_id IS NULL) AND (?::text IS NULL OR p.branch_id=?) AND (p.created_at<? OR (p.created_at=? AND p.id<?)) AND (?=0 OR EXISTS(SELECT 1 FROM social_follows f WHERE f.principal_id=? AND f.target_type='post' AND f.target_id=p.id)) AND (?::text IS NULL OR EXISTS(SELECT 1 FROM social_post_topics pt WHERE pt.post_id=p.id AND pt.tag=?) OR EXISTS(SELECT 1 FROM social_comment_topics ct JOIN social_comments c ON c.id=ct.comment_id WHERE c.post_id=p.id AND c.state='published' AND ct.tag=?))
        AND (?::text IS NULL OR p.purpose=?) AND (?::text IS NULL OR NOT EXISTS(SELECT 1 FROM social_post_sources s WHERE s.post_id=p.id AND s.source_kind='pms_record')
          OR EXISTS(SELECT 1 FROM social_post_sources s JOIN (${pmsSourcesSql}) pr ON pr.id=s.source_id
            WHERE s.post_id=p.id AND s.source_kind='pms_record' AND pr.company_id=p.company_id AND pr.property_id=?))
        ORDER BY p.created_at DESC,p.id DESC LIMIT ?`,
    )
    .bind(
      actor.companyId,
      actor.userId,
      now,
      now,
      actor.assignmentId,
      actor.branchId,
      actor.branchId,
      filter.branchId ?? null,
      filter.branchId ?? null,
      filter.before ?? now + 1,
      filter.before ?? now + 1,
      filter.beforeId ?? "",
      filter.following ? 1 : 0,
      actor.userId,
      filter.tag ?? null, filter.tag ?? null, filter.tag ?? null,
      filter.purpose ?? null, filter.purpose ?? null, filter.propertyId ?? null, filter.propertyId ?? null,
      limit,
    )
    .all<{ id: string }>();
  return postViews(db,actor,result.results.map(row=>row.id),2);
}
export async function postRevisionHistory(db: SqlDatabase, actor: StaffActor, id: string) {
  const now = Date.now();
  const post = await db.prepare(`SELECT p.author_id FROM social_posts p WHERE p.id=? AND p.company_id=? AND ${sourceAccessSql()}`)
    .bind(id, actor.companyId, actor.userId, now, now).first<{ author_id: string }>();
  if (!post || post.author_id !== actor.userId)
    throw new ApiError(404, "POST_NOT_FOUND", "This post is unavailable.");
  const rows = await db.prepare(`SELECT prior_version,action,body,created_at FROM social_post_revisions
    WHERE post_id=? ORDER BY prior_version DESC`).bind(id)
    .all<{ prior_version: number; action: "edit" | "retract"; body: string; created_at: number }>();
  return rows.results;
}
export async function postAction(
  db: SqlDatabase,
  actor: Actor,
  id: string,
  input: {
    action: "comment" | "delete_comment" | "edit_comment" | "appreciate" | "follow" | "report" | "hide" | "edit" | "retract";
    body?: string;
    content?: PostContent;
    expectedVersion?: number;
    active?: boolean;
    mentionUserIds?: string[];
    mentions?: Mention[];
    richContent?: unknown;
    parentCommentId?: string;
    commentId?: string;
    commentDraftId?: string;
    editDraftId?: string;
  },
  key: string,
) {
  const operation = `social.post.${input.action}`;
  const replayed = await replayCommand<{ id: string; version?: number }>(
    db,
    actor,
    key,
    operation,
    id,
    input,
  );
  if (replayed) return replayed;
  const post = await readablePost(db, actor, id),
    now = Date.now(),
    steps = [sourceGuard(db, key, post, actor.userId, "source")];
  if (input.action === "delete_comment" || input.action === "edit_comment") {
    if (!input.commentId) throw new ApiError(422, "COMMENT_ID_REQUIRED", "Choose a comment to delete.");
    const comment = await db.prepare("SELECT author_id,author_kind,state,version,rich_content_json,mentions_json FROM social_comments WHERE id=? AND post_id=?")
      .bind(input.commentId, id).first<{ author_id: string; author_kind: string; state: string; version: number; rich_content_json: string | null; mentions_json: string }>();
    if (!comment || comment.state !== "published")
      throw new ApiError(404, "COMMENT_NOT_FOUND", "This comment is unavailable.");
    const ownAiReply = comment.author_id !== actor.userId && await db.prepare(
      "SELECT 1 FROM social_agent_replies WHERE post_id=? AND actor_id=? AND ('reply-' || id)=? AND state='completed'",
    ).bind(id, actor.userId, input.commentId).first();
    if (comment.author_id !== actor.userId && (input.action === "edit_comment" || !ownAiReply))
      throw new ApiError(403, "COMMENT_AUTHOR_REQUIRED", "Only the author can delete this comment.");
    if (input.action === "edit_comment") {
      if (comment.author_kind === "agent") throw new ApiError(403, "AI_COMMENT_EDIT_FORBIDDEN", "AI replies cannot be edited.");
      const body = z.string().trim().min(1).max(10000).parse(input.body);
      let rich: RichContent;
      try {
        const parsed = validateRichContent(input.richContent, 10000);
        if (parsed.body !== body || JSON.stringify(parsed.mentions) !== JSON.stringify(input.mentions ?? []))
          throw Error("Rich text and mentions differ from the message.");
        rich = parsed.content;
      } catch (error) {
        throw new ApiError(422, "RICH_CONTENT_INVALID", error instanceof Error ? error.message : "Invalid rich content.");
      }
      const previousImages = (comment.rich_content_json ? (JSON.parse(comment.rich_content_json) as RichContent).blocks : [])
        .filter(block => block.type === "image");
      if (JSON.stringify(rich.blocks.filter(block => block.type === "image")) !== JSON.stringify(previousImages))
        throw new ApiError(422, "COMMENT_IMAGES_CHANGED", "Comment images cannot be changed while editing text.");
      const mentions = await validateMentions(db, actor, body, input.mentions);
      const agentMentions = mentions.filter(mention => mention.kind === "agent");
      const previousAgentMentions = (JSON.parse(comment.mentions_json) as Mention[]).filter(mention => mention.kind === "agent");
      if (JSON.stringify(agentMentions) !== JSON.stringify(previousAgentMentions))
        throw new ApiError(422, "COMMENT_AI_MENTION_EDIT", "AI requests cannot be changed while editing a comment.");
      steps.push(assertion(db, key, "comment-author",
        "EXISTS(SELECT 1 FROM social_comments WHERE id=? AND post_id=? AND author_id=? AND author_kind='person' AND state='published' AND version=?)",
        [input.commentId, id, actor.userId, comment.version]));
      steps.push(db.prepare("DELETE FROM social_mentions WHERE owner_type='comment' AND owner_id=?").bind(input.commentId));
      steps.push(db.prepare("DELETE FROM social_comment_topics WHERE comment_id=?").bind(input.commentId));
      steps.push(db.prepare("UPDATE social_comments SET body=?,rich_content_json=?,mentions_json=?,version=version+1,updated_at=?,last_command_id=? WHERE id=? AND post_id=? AND author_id=? AND state='published' AND version=?")
        .bind(body, JSON.stringify(rich), JSON.stringify(mentions), now, key, input.commentId, id, actor.userId, comment.version));
      steps.push(...await textStatements(db, actor, id, "comment", input.commentId, body, mentions.filter(mention => mention.kind !== "agent")));
      for (const mention of agentMentions) steps.push(db.prepare("INSERT INTO social_mentions(owner_type,owner_id,target_kind,target_id,target_label,created_by,created_at) VALUES('comment',?,?,?,?,?,?)")
        .bind(input.commentId, mention.kind, mention.targetId, mention.label, actor.userId, now));
      return executeCommand(db, actor, key, operation, input, id, post.version + 1,
        { id: input.commentId, version: comment.version + 1 }, steps);
    }
    steps.push(assertion(db, key, "comment-author",
      "EXISTS(SELECT 1 FROM social_comments c WHERE c.id=? AND c.post_id=? AND c.state='published' AND c.version=? AND (c.author_id=? OR EXISTS(SELECT 1 FROM social_agent_replies r WHERE r.post_id=c.post_id AND r.actor_id=? AND ('reply-' || r.id)=c.id AND r.state='completed')))",
      [input.commentId, id, comment.version, actor.userId, actor.userId]));
    steps.push(db.prepare("UPDATE social_comments SET state='tombstoned',version=version+1,updated_at=?,last_command_id=? WHERE id=? AND post_id=? AND state='published' AND version=?")
      .bind(now, key, input.commentId, id, comment.version));
    return executeCommand(db, actor, key, operation, input, id, post.version + 1,
      { id: input.commentId }, steps);
  }
  if (input.action === "edit" || input.action === "retract") {
    if (post.author_id !== actor.userId || post.kind !== "team" || post.linked_module)
      throw new ApiError(403, "POST_AUTHOR_REQUIRED", "Only the author can change this team post.");
    // AI replies and other comments advance the parent version; deletion uses
    // the latest row read above so an older feed snapshot can still be removed.
    if (input.action === "edit" && input.expectedVersion !== post.version)
      throw new ApiError(409, "POST_CHANGED", "This post changed. Reload it before trying again.");
    const content = storedContent(post.content_json);
    if(input.action==="edit"&&post.rich_content_json&&input.richContent===undefined)
      throw new ApiError(422,"RICH_CONTENT_REQUIRED","Reload this post in the advanced editor before changing its text.");
    steps.push(db.prepare(`INSERT INTO social_post_revisions
      (id,post_id,prior_version,action,body,content_json,rich_content_json,mentions_json,actor_id,created_at)
      VALUES(?,?,?,?,?,?,?,?,?,?)`)
      .bind(`revision-${key}`, id, post.version, input.action, post.body,
        post.content_json, post.rich_content_json, post.mentions_json, actor.userId, now));
    if (input.action === "retract") {
      steps.push(db.prepare("UPDATE social_posts SET state='archived',retracted_at=?,version=version+1,updated_at=? WHERE id=? AND author_id=? AND version=?")
        .bind(now, now, id, actor.userId, post.version));
    } else {
      const body = z.string().max(30000).parse(input.body);
      const editedContent = postContentSchema.parse(input.content ?? [
        { type: "text" as const, text: body },
        ...(content ? contentImages(content) : []),
      ]);
      if (!body.trim() && !contentImages(editedContent).length)
        throw new ApiError(422, "POST_BODY_REQUIRED", "Add a message or image.");
      if (contentBody(editedContent) !== body)
        throw new ApiError(422, "POST_CONTENT_CHANGED", "The post text changed. Review the edit and try again.");
      const editedImages = contentImages(editedContent);
      if (editedImages.length > 10)
        throw new ApiError(422, "ATTACHMENT_LIMIT", "Posts accept up to ten images.");
      if (editedImages.some(image => !image.attachmentId) ||
        new Set(editedImages.map(image => image.slotId)).size !== editedImages.length ||
        new Set(editedImages.map(image => image.attachmentId)).size !== editedImages.length)
        throw new ApiError(422, "POST_IMAGE_INVALID", "Each post image needs a unique uploaded file.");
      let rich:RichContent|null=null;
      if (input.richContent !== undefined) {
        try {
          const parsed = validateRichContent(input.richContent, 30000); rich = parsed.content;
          if (parsed.body !== body || JSON.stringify(parsed.mentions) !== JSON.stringify(input.mentions ?? []))
            throw Error("Rich text and mentions differ from the message.");
          if (rich.blocks.length !== editedContent.length || rich.blocks.some((block, index) => {
            const ordered = editedContent[index];
            return block.type !== ordered.type || block.type === "image" && ordered.type === "image" &&
              (block.slotId !== ordered.slotId || block.attachmentId !== ordered.attachmentId ||
                block.name !== ordered.name || block.alt !== ordered.alt || block.width !== ordered.width);
          })) throw Error("Rich text and post content differ.");
        } catch (e) { throw new ApiError(422, "RICH_CONTENT_INVALID", e instanceof Error ? e.message : "Invalid rich content."); }
      } else if (post.rich_content_json) {
        throw new ApiError(422, "RICH_CONTENT_REQUIRED", "Reload this post in the editor before changing its content.");
      }
      const mentions = await validateMentions(db, actor, body, input.mentions);
      if (!rich) {
        let offset = 0;
        rich = { version: 1, blocks: editedContent.map(block => {
          if (block.type === "image") return { type: "image" as const, slotId: block.slotId, attachmentId: block.attachmentId,
            name: block.name, alt: block.alt, width: block.width };
          const local = mentions.filter(mention => mention.start >= offset && mention.end <= offset + block.text.length)
            .map(mention => ({ ...mention, start: mention.start - offset, end: mention.end - offset }));
          offset += block.text.length + 1;
          return { type: "text" as const, doc: (plainRichContent(block.text, local).blocks[0] as { doc: RichNode }).doc };
        }) };
      }
      const currentImageIds = new Set(content
        ? contentImages(content).flatMap(image => image.attachmentId ? [image.attachmentId] : [])
        : (await db.prepare(`SELECT a.id FROM attachments a JOIN social_post_drafts d ON d.id=a.owner_id
            WHERE d.published_post_id=? AND d.company_id=? AND a.owner_type='social_draft' AND a.revoked_at IS NULL`)
            .bind(id, actor.companyId).all<{ id: string }>()).results.map(image => image.id));
      const retained = editedImages.filter(image => currentImageIds.has(image.attachmentId!));
      const newlyUploaded = editedImages.filter(image => !currentImageIds.has(image.attachmentId!));
      if (newlyUploaded.length && !input.editDraftId)
        throw new ApiError(422, "POST_IMAGE_UPLOAD_REQUIRED", "Upload new images before saving this edit.");
      let editDraft: PostDraft | null = null;
      if (input.editDraftId) {
        editDraft = await socialUploadOwner(db, actor, input.editDraftId, true);
        if (editDraft.edit_post_id !== id || editDraft.edit_post_version !== post.version || editDraft.edit_saved_version !== null)
          throw new ApiError(422, "POST_IMAGE_UPLOAD_UNAVAILABLE", "These uploaded images belong to a different post edit.");
        if (!newlyUploaded.length)
          throw new ApiError(422, "POST_IMAGE_UPLOAD_UNUSED", "Remove the unused image upload and try again.");
      }
      const existingFound = retained.length ? (await db.prepare(`SELECT a.id FROM attachments a JOIN social_post_drafts d ON d.id=a.owner_id
        WHERE a.id IN (${retained.map(() => "?").join(",")}) AND a.owner_type='social_draft'
          AND (d.published_post_id=? OR (d.edit_post_id=? AND d.edit_saved_version IS NOT NULL))
          AND d.company_id=? AND a.revoked_at IS NULL AND a.media_type IN ('image/jpeg','image/png','image/webp')`)
        .bind(...retained.map(image => image.attachmentId), id, id, actor.companyId).all<{ id: string }>()).results : [];
      if (existingFound.length !== retained.length)
        throw new ApiError(422, "POST_IMAGE_UNAVAILABLE", "An existing image is no longer available.");
      const uploadedFound = newlyUploaded.length && editDraft ? (await db.prepare(`SELECT a.id FROM attachments a
        JOIN upload_intents u ON u.id=a.upload_id JOIN social_post_drafts d ON d.id=a.owner_id
        WHERE a.id IN (${newlyUploaded.map(() => "?").join(",")}) AND a.owner_type='social_draft' AND a.owner_id=?
          AND d.author_id=? AND d.company_id=? AND d.edit_post_id=? AND d.edit_post_version=? AND d.edit_saved_version IS NULL
          AND d.cancelled_at IS NULL AND a.revoked_at IS NULL AND a.media_type IN ('image/jpeg','image/png','image/webp')
          AND u.owner_type='social_draft' AND u.owner_id=d.id AND u.status='ready'`)
        .bind(...newlyUploaded.map(image => image.attachmentId), input.editDraftId, actor.userId, actor.companyId, id, post.version).all<{ id: string }>()).results : [];
      if (uploadedFound.length !== newlyUploaded.length)
        throw new ApiError(422, "POST_IMAGE_UNAVAILABLE", "An uploaded image is unavailable. Upload it again.");
      const priorMentions = JSON.parse(post.mentions_json ?? "[]") as Mention[];
      const newRecipients: string[] = [];
      for (const mention of mentions) {
        if (mention.kind !== "person" || priorMentions.some(previous => previous.kind === "person" && previous.targetId === mention.targetId)) continue;
        await readablePost(db, { companyId: actor.companyId, userId: mention.targetId }, id);
        if (!newRecipients.includes(mention.targetId)) newRecipients.push(mention.targetId);
      }
      const tags = [...new Set(hashtags(body).filter(tag => !mentions.some(mention =>
        tag.start < mention.end && tag.end > mention.start)).map(tag => tag.tag))];
      steps.push(
        ...(retained.map((image, index) => assertion(db, key, `existing-image-${index}`,
          "EXISTS(SELECT 1 FROM attachments a JOIN social_post_drafts d ON d.id=a.owner_id WHERE a.id=? AND a.owner_type='social_draft' AND (d.published_post_id=? OR (d.edit_post_id=? AND d.edit_saved_version IS NOT NULL)) AND d.company_id=? AND a.revoked_at IS NULL)",
          [image.attachmentId, id, id, actor.companyId]))),
        ...(newlyUploaded.map((image, index) => assertion(db, key, `new-image-${index}`,
          "EXISTS(SELECT 1 FROM attachments a JOIN upload_intents u ON u.id=a.upload_id JOIN social_post_drafts d ON d.id=a.owner_id WHERE a.id=? AND a.owner_type='social_draft' AND a.owner_id=? AND d.author_id=? AND d.company_id=? AND d.edit_post_id=? AND d.edit_post_version=? AND d.edit_saved_version IS NULL AND d.cancelled_at IS NULL AND a.revoked_at IS NULL AND u.owner_type='social_draft' AND u.owner_id=d.id AND u.status='ready')",
          [image.attachmentId, input.editDraftId!, actor.userId, actor.companyId, id, post.version]))),
        ...(editDraft && input.editDraftId ? [assertion(db, key, "edit-upload-draft",
          "EXISTS(SELECT 1 FROM social_post_drafts WHERE id=? AND author_id=? AND company_id=? AND edit_post_id=? AND edit_post_version=? AND edit_saved_version IS NULL AND cancelled_at IS NULL)",
          [input.editDraftId, actor.userId, actor.companyId, id, post.version])] : []),
        db.prepare("DELETE FROM social_mentions WHERE owner_type='post' AND owner_id=?").bind(id),
        db.prepare("DELETE FROM social_post_topics WHERE post_id=?").bind(id),
        db.prepare("UPDATE social_posts SET body=?,content_json=?,rich_content_json=?,mentions_json=?,edited_at=?,version=version+1,updated_at=? WHERE id=? AND author_id=? AND version=?")
          .bind(body, JSON.stringify(editedContent),
            rich?JSON.stringify(rich):null,JSON.stringify(mentions), now, now, id, actor.userId, post.version),
        ...(editDraft && input.editDraftId ? [db.prepare("UPDATE social_post_drafts SET edit_saved_version=?,version=version+1,updated_at=? WHERE id=? AND edit_saved_version IS NULL AND cancelled_at IS NULL")
          .bind(post.version + 1, now, input.editDraftId)] : []),
        db.prepare(`UPDATE attachments SET revoked_at=? WHERE owner_type='social_draft' AND revoked_at IS NULL
          AND owner_id IN (SELECT id FROM social_post_drafts WHERE published_post_id=? OR edit_post_id=?)
          ${editedImages.length ? `AND id NOT IN (${editedImages.map(() => "?").join(",")})` : ""}`)
          .bind(now, id, id, ...editedImages.map(image => image.attachmentId)),
        db.prepare(`UPDATE upload_intents u SET status='expired',updated_at=? WHERE u.owner_type='social_draft' AND u.status='ready'
          AND u.owner_id IN (SELECT id FROM social_post_drafts WHERE published_post_id=? OR edit_post_id=?)
          AND NOT EXISTS(SELECT 1 FROM attachments a WHERE a.upload_id=u.id AND a.revoked_at IS NULL)`)
          .bind(now, id, id),
        ...mentions.map(mention => db.prepare(`INSERT INTO social_mentions
          (owner_type,owner_id,target_kind,target_id,target_label,created_by,created_at)
          VALUES('post',?,?,?,?,?,?)`)
          .bind(id, mention.kind, mention.targetId, mention.label, actor.userId, now)),
        ...tags.flatMap(tag => [
          db.prepare("INSERT INTO social_topics(tag,created_by,created_at) VALUES(?,?,?) ON CONFLICT DO NOTHING")
            .bind(tag, actor.userId, now),
          db.prepare("INSERT INTO social_post_topics(post_id,tag) VALUES(?,?) ON CONFLICT DO NOTHING")
            .bind(id, tag),
        ]),
        ...newRecipients.map(user => db.prepare(`INSERT INTO staff_notifications
          (id,company_id,user_id,record_type,record_id,record_version,message,created_at)
          SELECT ?,p.company_id,?,'post',p.id,p.version,'You were mentioned in an edited conversation',?
          FROM social_posts p WHERE p.id=? AND ${sourceAccessSql()}`)
          .bind(crypto.randomUUID(), user, now, id, user, now, now)),
      );
    }
  } else if (input.action === "comment") {
    const body = z.string().max(10000).parse(input.body);
    let rich:RichContent|null=null;
    if(input.richContent!==undefined){try{const parsed=validateRichContent(input.richContent,10000);rich=parsed.content;if(parsed.body!==body||JSON.stringify(parsed.mentions)!==JSON.stringify(input.mentions??[]))throw Error("Rich text and mentions differ from the message.");}catch(e){throw new ApiError(422,"RICH_CONTENT_INVALID",e instanceof Error?e.message:"Invalid rich content.");}}
    const images=rich?.blocks.filter(block=>block.type==="image")??[];
    if(!body.trim()&&!images.length)throw new ApiError(422,"COMMENT_BODY_REQUIRED","Add a message or image.");
    if(images.length>10||new Set(images.map(image=>image.attachmentId)).size!==images.length||images.some(image=>!image.attachmentId))throw new ApiError(422,"COMMENT_IMAGES_INVALID","Finish uploading up to ten unique images.");
    if(images.length&&!input.commentDraftId)throw new ApiError(422,"COMMENT_UPLOAD_REQUIRED","Upload images before replying.");
    if(input.commentDraftId){
      const upload=await socialUploadOwner(db,actor,input.commentDraftId,true);
      if(upload.comment_parent_post_id!==id)throw new ApiError(404,"COMMENT_UPLOAD_UNAVAILABLE","Comment images belong to another conversation.");
      const found=images.length?(await db.prepare(`SELECT a.id FROM attachments a JOIN upload_intents u ON u.id=a.upload_id JOIN social_post_drafts d ON d.id=a.owner_id WHERE a.id IN (${images.map(()=>"?").join(",")}) AND a.owner_type='social_draft' AND a.owner_id=? AND d.company_id=? AND a.revoked_at IS NULL AND u.status='ready' AND a.media_type IN ('image/jpeg','image/png','image/webp')`)
        .bind(...images.map(image=>image.attachmentId),input.commentDraftId,actor.companyId).all<{id:string}>()).results:[];
      if(found.length!==images.length)throw new ApiError(422,"COMMENT_IMAGE_UNAVAILABLE","An image is unavailable. Upload it again.");
      steps.push(assertion(db,key,"comment-upload","EXISTS(SELECT 1 FROM social_post_drafts WHERE id=? AND author_id=? AND comment_parent_post_id=? AND published_comment_id IS NULL AND cancelled_at IS NULL)",[input.commentDraftId,actor.userId,id]));
      for(const image of images)steps.push(assertion(db,key,`image-${image.slotId}`,"EXISTS(SELECT 1 FROM attachments a JOIN social_post_drafts d ON d.id=a.owner_id WHERE a.id=? AND a.owner_type='social_draft' AND a.owner_id=? AND d.company_id=? AND a.revoked_at IS NULL)",[image.attachmentId,input.commentDraftId,actor.companyId]));
    }
    const mentions = await validateMentions(db, actor, body, input.mentions);
    const parent = input.parentCommentId
      ? await db.prepare("SELECT id,author_id,parent_comment_id FROM social_comments WHERE id=? AND post_id=? AND state='published'")
          .bind(input.parentCommentId, id)
          .first<{ id: string; author_id: string; parent_comment_id: string | null }>()
      : null;
    if (input.parentCommentId && !parent)
      throw new ApiError(422, "REPLY_TARGET_UNAVAILABLE", "Choose a visible comment to reply to.");
    if (parent) steps.push(assertion(db, key, "reply-parent",
      "EXISTS(SELECT 1 FROM social_comments WHERE id=? AND post_id=? AND state='published')",
      [parent.id, id]));
    steps.push(
      db
        .prepare(
          "INSERT INTO social_comments(id,post_id,author_id,author_name,body,last_command_id,created_at,updated_at,mentions_json,parent_comment_id,rich_content_json,comment_upload_draft_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          `comment-${key}`,
          id,
          actor.userId,
          actor.name,
          body,
          key,
          now,
          now,
          JSON.stringify(mentions),
          parent?.id ?? null,
          rich?JSON.stringify(rich):null,
          input.commentDraftId??null,
        ),
      ...await textStatements(db, actor, id, "comment", `comment-${key}`, body, mentions),
    );
    if(input.commentDraftId)steps.push(db.prepare("UPDATE social_post_drafts SET published_comment_id=?,version=version+1,updated_at=? WHERE id=? AND published_comment_id IS NULL")
      .bind(`comment-${key}`,now,input.commentDraftId));
    if(input.commentDraftId)steps.push(db.prepare(`UPDATE attachments SET revoked_at=? WHERE owner_type='social_draft' AND owner_id=? AND revoked_at IS NULL${images.length?` AND id NOT IN (${images.map(()=>"?").join(",")})`:""}`)
      .bind(now,input.commentDraftId,...images.map(image=>image.attachmentId)));
    if (parent && parent.author_id !== actor.userId) steps.push(
      db.prepare(`INSERT INTO staff_notifications
        (id,company_id,user_id,record_type,record_id,record_version,message,created_at)
        SELECT ?,p.company_id,?,'post',p.id,p.version,'Someone replied to your comment',?
        FROM social_posts p WHERE p.id=? AND ${sourceAccessSql()}`)
        .bind(crypto.randomUUID(), parent.author_id, now, id, parent.author_id, now, now),
    );
    for (const [i, user] of [...new Set(input.mentionUserIds ?? [])]
      .slice(0, 10)
      .entries()) {
      await readablePost(db, { companyId: actor.companyId, userId: user }, id);
      steps.push(
        sourceGuard(
          db,
          key,
          { ...post, version: post.version + 1 },
          user,
          `mention-${i}`,
        ),
        notification(
          db,
          post,
          user,
          "post",
          id,
          post.version + 1,
          "You were mentioned in a conversation",
        ),
      );
    }
  } else if (input.action === "appreciate")
    steps.push(
      input.active
        ? db
            .prepare(
              "INSERT INTO social_reactions(post_id,principal_id,kind,created_at) VALUES(?,?,'appreciate',?) ON CONFLICT DO NOTHING",
            )
            .bind(id, actor.userId, now)
        : db
            .prepare(
              "DELETE FROM social_reactions WHERE post_id=? AND principal_id=?",
            )
            .bind(id, actor.userId),
    );
  else if (input.action === "follow")
    steps.push(
      input.active
        ? db
            .prepare(
              "INSERT INTO social_follows(principal_id,target_type,target_id,created_at) VALUES(?,'post',?,?) ON CONFLICT DO NOTHING",
            )
            .bind(actor.userId, id, now)
        : db
            .prepare(
              "DELETE FROM social_follows WHERE principal_id=? AND target_type='post' AND target_id=?",
            )
            .bind(actor.userId, id),
    );
  else if (input.action === "report")
    steps.push(
      db
        .prepare(
          "INSERT INTO social_reports(id,post_id,reporter_id,reason,created_at) VALUES(?,?,?,?,?) ON CONFLICT(post_id,reporter_id) DO UPDATE SET reason=excluded.reason",
        )
        .bind(
          `report-${key}`,
          id,
          actor.userId,
          z.string().trim().min(1).max(2000).parse(input.body),
          now,
        ),
    );
  else {
    await requireCompanyAuthority(db, actor, ["holding_owner"]);
    steps.push(
      authorityGuard(db, key, actor, ["holding_owner"]),
      db
        .prepare(
          "UPDATE social_posts SET state='moderated',version=version+1,updated_at=? WHERE id=?",
        )
        .bind(now, id),
    );
  }
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    id,
    post.version + 1,
    input.action === "edit" || input.action === "retract"
      ? { id, version: post.version + 1 }
      : { id },
    steps,
  );
}
export type HomePost = Awaited<ReturnType<typeof postView>>;
export type { PostRow };
