import type { SqlDatabase } from "./runtime/types";
import { ApiError } from "./http";
import { assertion } from "./commands";
import { sha256 } from "./crypto";
import type { StaffActor } from "./staff-policy";
import type { WorkContext } from "./authorization";
import { handoverSourcesReadableSql } from "./handover-policy";
export type PostRow = {
  id: string;
  kind: "team" | "record" | "system";
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  author_id: string;
  author_name: string;
  body: string;
  mentions_json: string;
  content_json: string | null;
  audience_kind: "company" | "branch" | "department" | "participants" | "team" | "project" | null;
  audience_id: string | null;
  purpose: "update" | "plan" | "blocker" | "handover" | "announcement";
  edited_at: number | null;
  retracted_at: number | null;
  version: number;
  state: string;
  linked_module: string | null;
  created_at: number;
};
export const sourceAccessSql = (alias = "p") =>
  `EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=? AND a.company_id=${alias}.company_id AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND (
    (${alias}.branch_id IS NULL AND ${alias}.department_id IS NULL)
    OR (${alias}.department_id IS NOT NULL AND a.scope_kind='department' AND a.scope_node_id=${alias}.department_id)
    OR (${alias}.department_id IS NULL AND ${alias}.branch_id IS NOT NULL AND (
      a.scope_kind='holding'
      OR (a.scope_kind='branch' AND a.scope_node_id=${alias}.branch_id)
      OR (a.scope_kind='department' AND EXISTS(SELECT 1 FROM organization_nodes n WHERE n.id=a.scope_node_id AND n.company_id=a.company_id AND n.branch_id=${alias}.branch_id))
    ))
  ) AND (
    ${alias}.audience_kind IS NULL OR ${alias}.audience_kind IN ('company','branch','department')
    OR (${alias}.audience_kind='participants' AND EXISTS(
      SELECT 1 FROM social_post_participants sp WHERE sp.post_id=${alias}.id AND sp.user_id=a.user_id))
    OR (${alias}.audience_kind='team' AND EXISTS(
      SELECT 1 FROM messaging_teams mt JOIN messaging_team_members mm ON mm.team_id=mt.id
      WHERE mt.id=${alias}.audience_id AND mt.company_id=${alias}.company_id
      AND NOT mt.organization_managed AND mm.user_id=a.user_id))
    OR (${alias}.audience_kind='project' AND EXISTS(
      SELECT 1 FROM staff_projects pr WHERE pr.id=${alias}.audience_id AND pr.company_id=${alias}.company_id
      AND (pr.owner_user_id=a.user_id OR EXISTS(
        SELECT 1 FROM staff_project_members pm WHERE pm.project_id=pr.id AND pm.user_id=a.user_id))))
  ) AND ${handoverSourcesReadableSql(alias,"a.user_id")})`;
/** The social feed and Executive activity adapter share the same published-post boundary. */
export function socialPostScope(actor: Pick<WorkContext,"companyId"|"userId"|"branchId"|"departmentId">, alias="p") {
  const now=Date.now(),base=`${alias}.company_id=? AND ${alias}.state='published' AND ${sourceAccessSql(alias)}`;
  return actor.departmentId
    ? {sql:`${base} AND (${alias}.department_id=? OR (${alias}.department_id IS NULL AND (${alias}.branch_id IS NULL OR ${alias}.branch_id=?)))`,bindings:[actor.companyId,actor.userId,now,now,actor.departmentId,actor.branchId]}
    : {sql:`${base} AND (${alias}.branch_id IS NULL OR ?::text IS NULL OR ${alias}.branch_id=?)`,bindings:[actor.companyId,actor.userId,now,now,actor.branchId,actor.branchId]};
}
export async function readablePost(
  db: SqlDatabase,
  actor: Pick<StaffActor, "userId" | "companyId">,
  id: string,
) {
  const now = Date.now();
  const row = await db
    .prepare(
      `SELECT p.* FROM social_posts p WHERE p.id=? AND p.company_id=? AND p.state='published' AND ${sourceAccessSql()}`,
    )
    .bind(id, actor.companyId, actor.userId, now, now)
    .first<PostRow>();
  if (!row)
    throw new ApiError(404, "POST_NOT_FOUND", "This post is unavailable.");
  return row;
}
export function sourceGuard(
  db: SqlDatabase,
  key: string,
  post: PostRow,
  user: string,
  step: string,
) {
  const now = Date.now();
  return assertion(
    db,
    key,
    step,
    `EXISTS(SELECT 1 FROM social_posts p WHERE p.id=? AND p.company_id=? AND p.version=? AND p.state='published' AND ${sourceAccessSql()})`,
    [post.id, post.company_id, post.version, user, now, now],
  );
}
export async function sourceSnapshot(
  db: SqlDatabase,
  actor: Pick<StaffActor, "userId" | "companyId">,
  id: string,
  commentIds: string[],
) {
  const post = await readablePost(db, actor, id);
  if (post.linked_module)
    throw new ApiError(
      422,
      "SOURCE_UNSUPPORTED",
      "Create a standalone team post for this workflow. Linked business records keep their existing permissions.",
    );
  const ids = [...new Set(commentIds)].sort();
  if (ids.length > 20)
    throw new ApiError(422, "SOURCE_LIMIT", "Select up to 20 comments.");
  const comments = ids.length
    ? (
        await db
          .prepare(
            `SELECT id,body,author_name,version FROM social_comments WHERE post_id=? AND state='published' AND id IN (${ids.map(() => "?").join(",")}) ORDER BY id`,
          )
          .bind(id, ...ids)
          .all<{
            id: string;
            body: string;
            author_name: string;
            version: number;
          }>()
      ).results
    : [];
  if (comments.length !== ids.length)
    throw new ApiError(
      409,
      "SOURCE_CHANGED",
      "A selected comment is unavailable. Review the conversation again.",
    );
  const sources = [
    {
      id: post.id,
      body: post.body,
      author: post.author_name,
      version: post.version,
    },
    ...comments.map((c) => ({
      id: c.id,
      body: c.body,
      author: c.author_name,
      version: c.version,
    })),
  ];
  const serialized = JSON.stringify(sources);
  if (new TextEncoder().encode(serialized).length > 18000)
    throw new ApiError(422, "SOURCE_LIMIT", "Select fewer comments.");
  return { post, sources, hash: await sha256(serialized), commentIds: ids };
}
