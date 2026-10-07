import type {z} from "zod";
import type {WorkContext} from "../../identity_access/public/authorization";
import type {ListPage} from "../../work_projection/public/list-page";
import {handbookSchema,handbookActionSchema,handbookEditorSchema,type Article,type HandbookRevision,type HandbookListRow} from "../domain/handbook";
export interface OwnerCommands<Effect>{replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;commit<Result>(actor:WorkContext,key:string,operation:string,input:unknown,id:string,version:number,result:Result,effects:Effect[],scope?:Pick<WorkContext,'companyId'|'branchId'|'departmentId'>):Promise<Result>;}
export type HandbookRepository<Effect>={editor(companyId:string,userId:string):Promise<Record<string,unknown>|null>;
settings(companyId:string):Promise<{ version: number }|null>;
editors(companyId:string):Promise<{results:{ user_id: string; granted_at: number }[]}>;
editorVersion(key:string,value:"editor-version",companyId:string,expectedVersion:number):Effect;
saveSettings(companyId:string,version:number):Effect;
grantEditor(companyId:string,userId:string,userId2:string,now:number):Effect;
removeEditor(companyId:string,userId:string):Effect;
article(id:string,companyId:string):Promise<Article|null>;
revision(id:string,companyId:string,value:number|null,value2:0 | 1):Promise<HandbookRevision|null>;
history(id:string,companyId:string,value:0 | 1):Promise<{results:Omit<HandbookRevision, "body">[]}>;
currentArticle(id:string,companyId:string):Promise<Article|null>;
articleVersion(key:string,value:"article-version",recordId:string,companyId:string,expectedVersion:number):Effect;
updateArticle(revision:number,version:number,now:number,recordId:string,companyId:string):Effect;
createArticle(recordId:string,companyId:string,version:number,revision:number,userId:string,now:number,now2:number):Effect;
createRevision(companyId:string,recordId:string,revision:number,title:string,category:"policy" | "procedure" | "faq" | "service_guide",body:string,userId:string,now:number):Effect;
actionArticle(id:string,companyId:string):Promise<Article|null>;
actionVersion(key:string,value:"article-version",id:string,companyId:string,version:number):Effect;
publishDraft(id:string,companyId:string,draft_revision:number):Promise<{ title: string; body: string }|null>;
publishRevision(userId:string,now:number,id:string,draft_revision:number,companyId:string):Effect;
publishArticle(version:number,now:number,id:string,companyId:string):Effect;
archiveArticle(value:0 | 1,version:number,now:number,id:string,companyId:string):Effect;};
export type HandbookPorts<Effect>={repository:HandbookRepository<Effect>;commands:OwnerCommands<Effect>;now():number;error(status:number,code:string,message:string):Error;companyMember(company:string,user:string):Promise<boolean>;requireServiceOwner(actor:WorkContext):Promise<void>;serviceOwner(actor:WorkContext):Promise<boolean>;serviceAuthoritySnapshot(company:string):Promise<(key:string)=>Effect[]>;authorityGuard(key:string,actor:WorkContext,roles:string[]):Effect;page(actor:WorkContext,q:string,editing:boolean,cursor?:string):Promise<ListPage<HandbookListRow>>;};
export function createHandbookService<Effect>(ports:HandbookPorts<Effect>){
async function handbookAccess(actor: WorkContext) {
  if (!(await ports.companyMember(actor.companyId, actor.userId)))
    throw ports.error(
      403,
      "MEMBERSHIP_REQUIRED",
      "Current company membership is required.",
    );
  const owner = await ports.serviceOwner(actor),
    editor =
      owner ||
      !!(await ports.repository.editor(actor.companyId,actor.userId));
  return { owner, editor };
}
async function requireEditor(actor: WorkContext) {
  if (!(await handbookAccess(actor)).editor)
    throw ports.error(
      403,
      "HANDBOOK_EDITOR_REQUIRED",
      "An owner-appointed handbook editor is required.",
    );
}
async function handbookEditorOptions(
  actor: WorkContext,
) {
  await ports.requireServiceOwner(actor);
  return {
    version:
      (
        await ports.repository.settings(actor.companyId)
      )?.version ?? 0,
    editors: (
      await ports.repository.editors(actor.companyId)
    ).results,
  };
}
async function setHandbookEditor(
  actor: WorkContext,
  input: z.infer<typeof handbookEditorSchema>,
  key: string,
) {
  const snap = await ports.serviceAuthoritySnapshot(actor.companyId);
  await ports.requireServiceOwner(actor);
  const operation = "handbook.editor.update",
    prior = await ports.commands.replay<{ id: string; version: number }>(
      actor,
      key,
      operation,
      input.userId,
      input,
    );
  if (prior) return prior;
  const options = await handbookEditorOptions(actor);
  if (options.version !== input.expectedVersion)
    throw ports.error(
      409,
      "VERSION_CONFLICT",
      "Editor access changed. Refresh before saving.",
    );
  if (input.active && !(await ports.companyMember(actor.companyId, input.userId)))
    throw ports.error(
      422,
      "EDITOR_MEMBER_REQUIRED",
      "Invite or activate this company member in People first.",
    );
  const version = options.version + 1;
  return ports.commands.commit(
    actor,
    key,
    operation,
    input,
    input.userId,
    version,
    { id: input.userId, version },
    [
      ...snap(key),
      ports.authorityGuard(key, actor, ["holding_owner"]),
      ports.repository.editorVersion(key,"editor-version",actor.companyId,input.expectedVersion),
      ports.repository.saveSettings(actor.companyId,version),
      input.active
        ? ports.repository.grantEditor(actor.companyId,input.userId,actor.userId,ports.now())
        : ports.repository.removeEditor(actor.companyId,input.userId),
    ],
  );
}
async function handbookList(actor:WorkContext,q="",editing=false,cursor?:string) {
  const access=await handbookAccess(actor);
  if(editing&&!access.editor)throw ports.error(403,"HANDBOOK_EDITOR_REQUIRED","Draft articles are available only to editors.");
  const page=await ports.page(actor,q,editing,cursor);
  return {access,...page,articles:page.items.map(r=>({...r,version:editing?r.version:r.revision}))};
}
async function handbookArticle(
  actor: WorkContext,
  id: string,
  editing = false,
  requestedRevision?: number,
) {
  const access = await handbookAccess(actor);
  if (editing && !access.editor)
    throw ports.error(
      403,
      "HANDBOOK_EDITOR_REQUIRED",
      "Draft articles are available only to editors.",
    );
  const article = await ports.repository.article(id,actor.companyId);
  if (
    !article ||
    (!editing && (article.archived || !article.published_revision))
  )
    throw ports.error(
      404,
      "ARTICLE_NOT_FOUND",
      "This article has not been published or is unavailable.",
    );
  if (
    requestedRevision !== undefined &&
    (!Number.isInteger(requestedRevision) || requestedRevision < 1)
  )
    throw ports.error(404, "ARTICLE_NOT_FOUND", "Article revision not found.");
  const revision = await ports.repository.revision(id,actor.companyId,requestedRevision ??
        (editing ? article.draft_revision : article.published_revision),editing ? 1 : 0);
  if (!revision)
    throw ports.error(
      404,
      "ARTICLE_NOT_FOUND",
      "This revision is not published or is unavailable.",
    );
  const history = (
    await ports.repository.history(id,actor.companyId,editing ? 1 : 0)
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
async function saveHandbookArticle(
  actor: WorkContext,
  id: string | null,
  input: z.infer<typeof handbookSchema>,
  key: string,
) {
  const snap = await ports.serviceAuthoritySnapshot(actor.companyId);
  await requireEditor(actor);
  const recordId = id ?? `article-${key}`,
    operation = id ? "handbook.article.update" : "handbook.article.create";
  const prior = await ports.commands.replay<{ id: string; version: number }>(
    actor,
    key,
    operation,
    recordId,
    input,
  );
  if (prior) return prior;
  const current = id
    ? await ports.repository.currentArticle(id,actor.companyId)
    : null;
  if (id && !current)
    throw ports.error(404, "ARTICLE_NOT_FOUND", "Article not found.");
  if (current && current.version !== input.expectedVersion)
    throw ports.error(
      409,
      "VERSION_CONFLICT",
      "This article changed. Review the latest draft before saving.",
    );
  const version = current ? current.version + 1 : 1,
    revision = current ? current.draft_revision + 1 : 1,
    now = ports.now();
  return ports.commands.commit(
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
            ports.repository.articleVersion(key,"article-version",recordId,actor.companyId,input.expectedVersion),
            ports.repository.updateArticle(revision,version,now,recordId,actor.companyId),
          ]
        : [
            ports.repository.createArticle(recordId,actor.companyId,version,revision,actor.userId,now,now),
          ]),
      ports.repository.createRevision(actor.companyId,recordId,revision,input.title,input.category,input.body,actor.userId,now),
    ],
  );
}
async function actOnHandbook(
  actor: WorkContext,
  id: string,
  input: z.infer<typeof handbookActionSchema>,
  key: string,
) {
  const snap = await ports.serviceAuthoritySnapshot(actor.companyId);
  await requireEditor(actor);
  const operation = `handbook.article.${input.action}`,
    prior = await ports.commands.replay<{ id: string; version: number }>(
      actor,
      key,
      operation,
      id,
      input,
    );
  if (prior) return prior;
  const row = await ports.repository.actionArticle(id,actor.companyId);
  if (!row) throw ports.error(404, "ARTICLE_NOT_FOUND", "Article not found.");
  if (row.version !== input.expectedVersion)
    throw ports.error(
      409,
      "VERSION_CONFLICT",
      "Review the current article before publishing or changing availability.",
    );
  const version = row.version + 1,
    now = ports.now(),
    statements = [
      ...snap(key),
      ports.repository.actionVersion(key,"article-version",id,actor.companyId,row.version),
    ];
  if (input.action === "publish") {
    const draft = await ports.repository.publishDraft(id,actor.companyId,row.draft_revision);
    if (!draft || draft.title.trim().length < 2 || draft.body.trim().length < 2)
      throw ports.error(
        422,
        "ARTICLE_CONTENT_REQUIRED",
        "Add a title and article content before publishing.",
      );
    statements.push(
      ports.repository.publishRevision(actor.userId,now,id,row.draft_revision,actor.companyId),
      ports.repository.publishArticle(version,now,id,actor.companyId),
    );
  } else
    statements.push(
      ports.repository.archiveArticle(input.action === "archive" ? 1 : 0,version,now,id,actor.companyId),
    );
  return ports.commands.commit(
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
return {handbookAccess,handbookEditorOptions,setHandbookEditor,handbookList,handbookArticle,saveHandbookArticle,actOnHandbook};}

