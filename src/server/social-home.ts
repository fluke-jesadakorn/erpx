import { readListPage } from "./list-page";
import type { WorkContext } from "./authorization";
import { hashtags, type Mention } from "@/lib/social-text";
import { mentionsSchema, validateMentions, textStatements } from "./social-text";
import type { SqlDatabase } from "./runtime/types";
import { z } from "zod";
import { ApiError } from "./http";
import { assertion, executeCommand, replayCommand } from "./commands";
import { getProject, getTask } from "./staff-tasks";
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
  z.object({ type: z.literal("text"), text: z.string().max(3000) }),
  z.object({
    type: z.literal("image"),
    slotId: z.string().min(1).max(160),
    attachmentId: z.string().min(1).max(160).nullable(),
    name: z.string().min(1).max(255),
    alt: z.string().max(500).optional(),
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
  kind: z.enum(["staff_task", "staff_project"]),
  id: z.string().min(1).max(160),
});
export type PostSource = z.infer<typeof postSourceSchema>;
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
  body: z.string().max(3000),
  content: postContentSchema.optional(),
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
export async function savePostDraft(
  db: SqlDatabase,
  actor: Actor,
  id: string | null,
  input: z.infer<typeof postDraftSchema>,
  key: string,
) {
  const recordId = id ?? `post-draft-${key}`,
    now = Date.now();
  if (id) await socialUploadOwner(db, actor, id, true);
  const content = input.content;
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
              "UPDATE social_post_drafts SET body=?,branch_id=?,department_id=?,audience_kind=?,audience_id=?,participants_json=?,sources_json=?,purpose=?,mentions_json=?,content_json=?,version=version+1,updated_at=? WHERE id=?",
            )
            .bind(input.body, branch, department, audience?.kind ?? null, audienceId, JSON.stringify(participants), JSON.stringify(sources), purpose, JSON.stringify(mentions), content ? JSON.stringify(content) : null, now, id)
        : db
            .prepare(
              "INSERT INTO social_post_drafts(id,author_id,company_id,branch_id,body,created_at,updated_at,department_id,audience_kind,audience_id,participants_json,sources_json,purpose,mentions_json,content_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
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
          "INSERT INTO social_posts(id,kind,author_id,author_name,company_id,branch_id,body,last_command_id,created_at,updated_at,department_id,audience_kind,audience_id,purpose,mentions_json,content_json) VALUES(?,'team',?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
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
type CommentRow = { id:string;post_id:string;body:string;author_id:string;author_name:string;created_at:number;mentions_json:string;author_kind:"person"|"agent";parent_comment_id:string|null;parent_author_name:string|null };
const commentView = (c:CommentRow) => ({...c,mentions:JSON.parse(c.mentions_json) as Mention[]});
export async function postComments(db:SqlDatabase,actor:StaffActor,id:string,cursor?:string) {
  await readablePost(db,actor,id);
  const page = await readListPage<CommentRow & {page_key:string}>(db,{
    sql:"SELECT c.id,c.post_id,c.body,c.author_id,c.author_name,c.created_at,c.mentions_json,c.author_kind,c.parent_comment_id,parent.author_name AS parent_author_name,c.created_at::text AS page_key FROM social_comments c LEFT JOIN social_comments parent ON parent.id=c.parent_comment_id AND parent.post_id=c.post_id WHERE c.post_id=? AND c.state='published'",
    args:[id],scope:[actor.companyId,actor.userId,id],cursor,limit:20,descending:true,
  });
  return {...page,items:page.items.map(commentView)};
}
async function postViews(db:SqlDatabase,actor:StaffActor,ids:string[],commentLimit:number) {
  if(!ids.length) return [];
  const now=Date.now(), slots=ids.map(()=>'?').join(',');
  const posts=(await db.prepare(`SELECT p.*,
    CASE WHEN p.audience_kind='team' THEN (SELECT t.name FROM messaging_teams t WHERE t.id=p.audience_id AND t.company_id=p.company_id)
      WHEN p.audience_kind='project' THEN (SELECT pr.title FROM staff_projects pr WHERE pr.id=p.audience_id AND pr.company_id=p.company_id)
      ELSE (SELECT n.display_name FROM organization_nodes n WHERE n.id=COALESCE(p.department_id,p.branch_id) AND n.company_id=p.company_id) END AS audience_name,
    (SELECT count(*) FROM social_post_participants pp WHERE pp.post_id=p.id AND pp.user_id<>p.author_id) AS audience_count
    FROM social_posts p WHERE p.id IN (${slots}) AND p.company_id=? AND p.state='published' AND ${sourceAccessSql()}`)
    .bind(...ids,actor.companyId,actor.userId,now,now).all<PostRow & {audience_name:string|null;audience_count:number}>()).results;
  if(!posts.length) return [];
  const allowed=posts.map(p=>p.id), placeholders=allowed.map(()=>'?').join(',');
  const [comments,files,counts,origins,replies,sources]=await Promise.all([
    db.prepare(`SELECT * FROM (SELECT c.id,c.post_id,c.body,c.author_id,c.author_name,c.created_at,c.mentions_json,c.author_kind,c.parent_comment_id,parent.author_name AS parent_author_name,
      row_number() OVER(PARTITION BY c.post_id ORDER BY c.created_at DESC,c.id DESC) ordinal
      FROM social_comments c LEFT JOIN social_comments parent ON parent.id=c.parent_comment_id AND parent.post_id=c.post_id
      WHERE c.post_id IN (${placeholders}) AND c.state='published') c WHERE ordinal<=? ORDER BY created_at,id`)
      .bind(...allowed,commentLimit).all<CommentRow>(),
    db.prepare(`SELECT d.published_post_id AS post_id,a.id,a.original_name,a.media_type FROM attachments a JOIN social_post_drafts d ON d.id=a.owner_id
      WHERE a.owner_type='social_draft' AND d.published_post_id IN (${placeholders}) AND a.revoked_at IS NULL`).bind(...allowed)
      .all<{post_id:string;id:string;original_name:string;media_type:string}>(),
    db.prepare(`SELECT p.id,(SELECT count(*) FROM social_comments WHERE post_id=p.id AND state='published') AS commentCount,
      (SELECT count(*) FROM social_reactions WHERE post_id=p.id) AS appreciationCount,
      EXISTS(SELECT 1 FROM social_reactions WHERE post_id=p.id AND principal_id=?) AS appreciated,
      EXISTS(SELECT 1 FROM social_follows WHERE target_type='post' AND target_id=p.id AND principal_id=?) AS followed
      FROM social_posts p WHERE p.id IN (${placeholders})`).bind(actor.userId,actor.userId,...allowed)
      .all<{id:string;commentCount:number;appreciationCount:number;appreciated:number;followed:number}>(),
    db.prepare(`SELECT post_id,task_id FROM social_task_origins WHERE post_id IN (${placeholders})`).bind(...allowed).all<{post_id:string;task_id:string}>(),
    db.prepare(`SELECT post_id,id,state,error,actor_id FROM social_agent_replies WHERE post_id IN (${placeholders}) ORDER BY created_at`).bind(...allowed)
      .all<{post_id:string;id:string;state:string;error:string|null;actor_id:string}>(),
    db.prepare(`SELECT post_id,source_kind,source_id,ordinal FROM social_post_sources WHERE post_id IN (${placeholders}) ORDER BY post_id,ordinal`)
      .bind(...allowed).all<{post_id:string;source_kind:PostSource["kind"];source_id:string;ordinal:number}>(),
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
    return {...post,audienceLabel,content,mentions:JSON.parse(post.mentions_json??'[]') as Mention[],
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
  } = {},
) {
  const now = Date.now();
  const limit = Math.max(1, Math.min(filter.limit ?? 20, 20));
  const result = await db
    .prepare(
      `SELECT p.id FROM social_posts p WHERE p.company_id=? AND p.state='published' AND p.linked_module IS NULL AND ${sourceAccessSql()} AND (?::text IS NULL OR p.branch_id=? OR p.branch_id IS NULL) AND (?::text IS NULL OR p.branch_id=?) AND (p.created_at<? OR (p.created_at=? AND p.id<?)) AND (?=0 OR EXISTS(SELECT 1 FROM social_follows f WHERE f.principal_id=? AND f.target_type='post' AND f.target_id=p.id)) AND (?::text IS NULL OR EXISTS(SELECT 1 FROM social_post_topics pt WHERE pt.post_id=p.id AND pt.tag=?) OR EXISTS(SELECT 1 FROM social_comment_topics ct JOIN social_comments c ON c.id=ct.comment_id WHERE c.post_id=p.id AND c.state='published' AND ct.tag=?)) ORDER BY p.created_at DESC,p.id DESC LIMIT ?`,
    )
    .bind(
      actor.companyId,
      actor.userId,
      now,
      now,
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
    action: "comment" | "appreciate" | "follow" | "report" | "hide" | "edit" | "retract";
    body?: string;
    expectedVersion?: number;
    active?: boolean;
    mentionUserIds?: string[];
    mentions?: Mention[];
    parentCommentId?: string;
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
  if (input.action === "edit" || input.action === "retract") {
    if (post.author_id !== actor.userId || post.kind !== "team" || post.linked_module)
      throw new ApiError(403, "POST_AUTHOR_REQUIRED", "Only the author can change this team post.");
    if (input.expectedVersion !== post.version)
      throw new ApiError(409, "POST_CHANGED", "This post changed. Reload it before trying again.");
    const content = storedContent(post.content_json);
    if (input.action === "edit" && contentImages(content ?? []).length)
      throw new ApiError(422, "POST_IMAGE_EDIT_UNSUPPORTED", "Posts with images cannot be edited yet. Retract and share a corrected post.");
    steps.push(db.prepare(`INSERT INTO social_post_revisions
      (id,post_id,prior_version,action,body,content_json,mentions_json,actor_id,created_at)
      VALUES(?,?,?,?,?,?,?,?,?)`)
      .bind(`revision-${key}`, id, post.version, input.action, post.body,
        post.content_json, post.mentions_json, actor.userId, now));
    if (input.action === "retract") {
      steps.push(db.prepare("UPDATE social_posts SET state='archived',retracted_at=?,version=version+1,updated_at=? WHERE id=? AND author_id=? AND version=?")
        .bind(now, now, id, actor.userId, post.version));
    } else {
      const body = z.string().trim().min(1).max(3000).parse(input.body);
      const mentions = await validateMentions(db, actor, body, input.mentions);
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
        db.prepare("DELETE FROM social_mentions WHERE owner_type='post' AND owner_id=?").bind(id),
        db.prepare("DELETE FROM social_post_topics WHERE post_id=?").bind(id),
        db.prepare("UPDATE social_posts SET body=?,content_json=?,mentions_json=?,edited_at=?,version=version+1,updated_at=? WHERE id=? AND author_id=? AND version=?")
          .bind(body, content ? JSON.stringify([{ type: "text", text: body }]) : null,
            JSON.stringify(mentions), now, now, id, actor.userId, post.version),
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
    const body = z.string().min(1).max(1000).refine(b=>!!b.trim()).parse(input.body);
    const mentions = await validateMentions(db, actor, body, input.mentions);
    const parent = input.parentCommentId
      ? await db.prepare("SELECT id,author_id,parent_comment_id FROM social_comments WHERE id=? AND post_id=? AND state='published'")
          .bind(input.parentCommentId, id)
          .first<{ id: string; author_id: string; parent_comment_id: string | null }>()
      : null;
    if (input.parentCommentId && (!parent || parent.parent_comment_id))
      throw new ApiError(422, "REPLY_TARGET_UNAVAILABLE", "Choose a visible top-level comment to reply to.");
    if (parent) steps.push(assertion(db, key, "reply-parent",
      "EXISTS(SELECT 1 FROM social_comments WHERE id=? AND post_id=? AND state='published' AND parent_comment_id IS NULL)",
      [parent.id, id]));
    steps.push(
      db
        .prepare(
          "INSERT INTO social_comments(id,post_id,author_id,author_name,body,last_command_id,created_at,updated_at,mentions_json,parent_comment_id) VALUES(?,?,?,?,?,?,?,?,?,?)",
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
        ),
      ...await textStatements(db, actor, id, "comment", `comment-${key}`, body, mentions),
    );
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
