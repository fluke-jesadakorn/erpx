import { readListPage } from "./list-page";
import type { SqlDatabase } from "./runtime/types";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import {
  companyMember,
  requireServiceOwner,
  serviceAuthoritySnapshot,
  serviceOwner,
} from "./service-access";
import { authorityGuard } from "./staff-policy";

export const handbookSchema = z.object({
  title: z.string().trim().max(200).default(""),
  category: z
    .enum(["policy", "procedure", "faq", "service_guide"])
    .default("procedure"),
  body: z.string().max(40000).default(""),
  expectedVersion: z.number().int().positive().default(1),
});
export const handbookActionSchema = z.object({
  action: z.enum(["publish", "archive", "restore"]),
  expectedVersion: z.number().int().positive(),
});
export const handbookEditorSchema = z.object({
  userId: z.string().min(1).max(160),
  active: z.boolean(),
  expectedVersion: z.number().int().min(0),
});
type Article = {
  id: string;
  company_id: string;
  version: number;
  draft_revision: number;
  published_revision: number | null;
  archived: number;
  created_by: string;
  updated_at: number;
};
export type HandbookRevision = {
  revision: number;
  title: string;
  category: string;
  body: string;
  created_by: string;
  created_at: number;
  published_by: string | null;
  published_at: number | null;
};
export async function handbookAccess(db: SqlDatabase, actor: WorkContext) {
  if (!(await companyMember(db, actor.companyId, actor.userId)))
    throw new ApiError(
      403,
      "MEMBERSHIP_REQUIRED",
      "Current company membership is required.",
    );
  const owner = await serviceOwner(db, actor),
    editor =
      owner ||
      !!(await db
        .prepare(
          "SELECT 1 FROM handbook_editors WHERE company_id=? AND user_id=?",
        )
        .bind(actor.companyId, actor.userId)
        .first());
  return { owner, editor };
}
async function requireEditor(db: SqlDatabase, actor: WorkContext) {
  if (!(await handbookAccess(db, actor)).editor)
    throw new ApiError(
      403,
      "HANDBOOK_EDITOR_REQUIRED",
      "An owner-appointed handbook editor is required.",
    );
}
export async function handbookEditorOptions(
  db: SqlDatabase,
  actor: WorkContext,
) {
  await requireServiceOwner(db, actor);
  return {
    version:
      (
        await db
          .prepare("SELECT version FROM handbook_settings WHERE company_id=?")
          .bind(actor.companyId)
          .first<{ version: number }>()
      )?.version ?? 0,
    editors: (
      await db
        .prepare(
          "SELECT user_id,granted_at FROM handbook_editors WHERE company_id=? ORDER BY user_id",
        )
        .bind(actor.companyId)
        .all<{ user_id: string; granted_at: number }>()
    ).results,
  };
}
export async function setHandbookEditor(
  db: SqlDatabase,
  actor: WorkContext,
  input: z.infer<typeof handbookEditorSchema>,
  key: string,
) {
  const snap = await serviceAuthoritySnapshot(db, actor.companyId);
  await requireServiceOwner(db, actor);
  const operation = "handbook.editor.update",
    prior = await replayCommand<{ id: string; version: number }>(
      db,
      actor,
      key,
      operation,
      input.userId,
      input,
    );
  if (prior) return prior;
  const options = await handbookEditorOptions(db, actor);
  if (options.version !== input.expectedVersion)
    throw new ApiError(
      409,
      "VERSION_CONFLICT",
      "Editor access changed. Refresh before saving.",
    );
  if (input.active && !(await companyMember(db, actor.companyId, input.userId)))
    throw new ApiError(
      422,
      "EDITOR_MEMBER_REQUIRED",
      "Invite or activate this company member in People first.",
    );
  const version = options.version + 1;
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    input.userId,
    version,
    { id: input.userId, version },
    [
      ...snap(key),
      authorityGuard(db, key, actor, ["holding_owner"]),
      assertion(
        db,
        key,
        "editor-version",
        "COALESCE((SELECT version FROM handbook_settings WHERE company_id=?),0)=?",
        [actor.companyId, input.expectedVersion],
      ),
      db
        .prepare(
          "INSERT INTO handbook_settings(company_id,version) VALUES(?,?) ON CONFLICT(company_id) DO UPDATE SET version=excluded.version",
        )
        .bind(actor.companyId, version),
      input.active
        ? db
            .prepare(
              "INSERT INTO handbook_editors(company_id,user_id,granted_by,granted_at) VALUES(?,?,?,?) ON CONFLICT(company_id,user_id) DO UPDATE SET granted_by=excluded.granted_by,granted_at=excluded.granted_at",
            )
            .bind(actor.companyId, input.userId, actor.userId, Date.now())
        : db
            .prepare(
              "DELETE FROM handbook_editors WHERE company_id=? AND user_id=?",
            )
            .bind(actor.companyId, input.userId),
    ],
  );
}
export async function handbookList(db:SqlDatabase,actor:WorkContext,q="",editing=false,cursor?:string) {
  const access=await handbookAccess(db,actor);
  if(editing&&!access.editor)throw new ApiError(403,"HANDBOOK_EDITOR_REQUIRED","Draft articles are available only to editors.");
  const page=await readListPage<{id:string;page_key:string;version:number;archived:number;revision:number;title:string;category:string;excerpt:string;published_at:number|null}>(db,{
    sql:`SELECT a.id,a.version,a.archived,r.revision,r.title,r.title AS page_key,r.category,left(r.body,220) AS excerpt,r.published_at
      FROM handbook_articles a JOIN handbook_revisions r ON r.article_id=a.id AND r.company_id=a.company_id
        AND r.revision=CASE WHEN ?=1 THEN a.draft_revision ELSE a.published_revision END
      WHERE a.company_id=? AND (?=1 OR a.archived=0) AND strpos(lower(r.title||' '||r.body||' '||r.category),lower(?))>0`,
    args:[editing?1:0,actor.companyId,editing?1:0,q],cursor,scope:[actor.companyId,actor.userId,'handbook',editing,q],
  });
  return {access,...page,articles:page.items.map(r=>({...r,version:editing?r.version:r.revision}))};
}
export async function handbookArticle(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  editing = false,
  requestedRevision?: number,
) {
  const access = await handbookAccess(db, actor);
  if (editing && !access.editor)
    throw new ApiError(
      403,
      "HANDBOOK_EDITOR_REQUIRED",
      "Draft articles are available only to editors.",
    );
  const article = await db
    .prepare("SELECT * FROM handbook_articles WHERE id=? AND company_id=?")
    .bind(id, actor.companyId)
    .first<Article>();
  if (
    !article ||
    (!editing && (article.archived || !article.published_revision))
  )
    throw new ApiError(
      404,
      "ARTICLE_NOT_FOUND",
      "This article has not been published or is unavailable.",
    );
  if (
    requestedRevision !== undefined &&
    (!Number.isInteger(requestedRevision) || requestedRevision < 1)
  )
    throw new ApiError(404, "ARTICLE_NOT_FOUND", "Article revision not found.");
  const revision = await db
    .prepare(
      "SELECT * FROM handbook_revisions WHERE article_id=? AND company_id=? AND revision=? AND (?=1 OR published_at IS NOT NULL)",
    )
    .bind(
      id,
      actor.companyId,
      requestedRevision ??
        (editing ? article.draft_revision : article.published_revision),
      editing ? 1 : 0,
    )
    .first<HandbookRevision>();
  if (!revision)
    throw new ApiError(
      404,
      "ARTICLE_NOT_FOUND",
      "This revision is not published or is unavailable.",
    );
  const history = (
    await db
      .prepare(
        "SELECT revision,title,category,created_by,created_at,published_by,published_at FROM handbook_revisions WHERE article_id=? AND company_id=? AND (?=1 OR published_at IS NOT NULL) ORDER BY revision DESC LIMIT 100",
      )
      .bind(id, actor.companyId, editing ? 1 : 0)
      .all<Omit<HandbookRevision, "body">>()
  ).results;
  return {
    access,
    article: editing
      ? article
      : {
          id: article.id,
          version: article.published_revision,
          archived: 0,
          published_revision: article.published_revision,
        },
    revision: revision!,
    history,
  };
}
export async function saveHandbookArticle(
  db: SqlDatabase,
  actor: WorkContext,
  id: string | null,
  input: z.infer<typeof handbookSchema>,
  key: string,
) {
  const snap = await serviceAuthoritySnapshot(db, actor.companyId);
  await requireEditor(db, actor);
  const recordId = id ?? `article-${key}`,
    operation = id ? "handbook.article.update" : "handbook.article.create";
  const prior = await replayCommand<{ id: string; version: number }>(
    db,
    actor,
    key,
    operation,
    recordId,
    input,
  );
  if (prior) return prior;
  const current = id
    ? await db
        .prepare("SELECT * FROM handbook_articles WHERE id=? AND company_id=?")
        .bind(id, actor.companyId)
        .first<Article>()
    : null;
  if (id && !current)
    throw new ApiError(404, "ARTICLE_NOT_FOUND", "Article not found.");
  if (current && current.version !== input.expectedVersion)
    throw new ApiError(
      409,
      "VERSION_CONFLICT",
      "This article changed. Review the latest draft before saving.",
    );
  const version = current ? current.version + 1 : 1,
    revision = current ? current.draft_revision + 1 : 1,
    now = Date.now();
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    recordId,
    version,
    { id: recordId, version },
    [
      ...snap(key),
      ...(current
        ? [
            assertion(
              db,
              key,
              "article-version",
              "EXISTS(SELECT 1 FROM handbook_articles WHERE id=? AND company_id=? AND version=?)",
              [recordId, actor.companyId, input.expectedVersion],
            ),
            db
              .prepare(
                "UPDATE handbook_articles SET draft_revision=?,version=?,updated_at=? WHERE id=? AND company_id=?",
              )
              .bind(revision, version, now, recordId, actor.companyId),
          ]
        : [
            db
              .prepare(
                "INSERT INTO handbook_articles(id,company_id,version,draft_revision,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?)",
              )
              .bind(
                recordId,
                actor.companyId,
                version,
                revision,
                actor.userId,
                now,
                now,
              ),
          ]),
      db
        .prepare(
          "INSERT INTO handbook_revisions(company_id,article_id,revision,title,category,body,created_by,created_at) VALUES(?,?,?,?,?,?,?,?)",
        )
        .bind(
          actor.companyId,
          recordId,
          revision,
          input.title,
          input.category,
          input.body,
          actor.userId,
          now,
        ),
    ],
  );
}
export async function actOnHandbook(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  input: z.infer<typeof handbookActionSchema>,
  key: string,
) {
  const snap = await serviceAuthoritySnapshot(db, actor.companyId);
  await requireEditor(db, actor);
  const operation = `handbook.article.${input.action}`,
    prior = await replayCommand<{ id: string; version: number }>(
      db,
      actor,
      key,
      operation,
      id,
      input,
    );
  if (prior) return prior;
  const row = await db
    .prepare("SELECT * FROM handbook_articles WHERE id=? AND company_id=?")
    .bind(id, actor.companyId)
    .first<Article>();
  if (!row) throw new ApiError(404, "ARTICLE_NOT_FOUND", "Article not found.");
  if (row.version !== input.expectedVersion)
    throw new ApiError(
      409,
      "VERSION_CONFLICT",
      "Review the current article before publishing or changing availability.",
    );
  const version = row.version + 1,
    now = Date.now(),
    statements = [
      ...snap(key),
      assertion(
        db,
        key,
        "article-version",
        "EXISTS(SELECT 1 FROM handbook_articles WHERE id=? AND company_id=? AND version=?)",
        [id, actor.companyId, row.version],
      ),
    ];
  if (input.action === "publish") {
    const draft = await db
      .prepare(
        "SELECT title,body FROM handbook_revisions WHERE article_id=? AND company_id=? AND revision=?",
      )
      .bind(id, actor.companyId, row.draft_revision)
      .first<{ title: string; body: string }>();
    if (!draft || draft.title.trim().length < 2 || draft.body.trim().length < 2)
      throw new ApiError(
        422,
        "ARTICLE_CONTENT_REQUIRED",
        "Add a title and article content before publishing.",
      );
    statements.push(
      db
        .prepare(
          "UPDATE handbook_revisions SET published_by=COALESCE(published_by,?),published_at=COALESCE(published_at,?) WHERE article_id=? AND revision=? AND company_id=?",
        )
        .bind(actor.userId, now, id, row.draft_revision, actor.companyId),
      db
        .prepare(
          "UPDATE handbook_articles SET published_revision=draft_revision,archived=0,version=?,updated_at=? WHERE id=? AND company_id=?",
        )
        .bind(version, now, id, actor.companyId),
    );
  } else
    statements.push(
      db
        .prepare(
          "UPDATE handbook_articles SET archived=?,version=?,updated_at=? WHERE id=? AND company_id=?",
        )
        .bind(
          input.action === "archive" ? 1 : 0,
          version,
          now,
          id,
          actor.companyId,
        ),
    );
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    id,
    version,
    { id, version },
    statements,
  );
}
