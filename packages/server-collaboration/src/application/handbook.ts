import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {ListPage} from "@erpx/server-work_projection/public/list-page";
import {type Article,type HandbookRevision,type HandbookListRow} from "@erpx/server-collaboration/domain/handbook";
import type {HandbookPurchaseControls} from '@erpx/server-collaboration/domain/handbook-purchases';
export type HandbookRepository={editor(companyId:string,userId:string):Promise<Record<string,unknown>|null>;
settings(companyId:string):Promise<{ version: number }|null>;
editors(companyId:string):Promise<{results:{ user_id: string; granted_at: number }[]}>;
article(id:string,companyId:string):Promise<Article|null>;
revision(id:string,companyId:string,value:number|null,value2:0 | 1):Promise<HandbookRevision|null>;
history(id:string,companyId:string,value:0 | 1):Promise<{results:Omit<HandbookRevision, "body">[]}>;};
export type HandbookPorts={repository:HandbookRepository;purchaseAccess(actor:WorkContext):Promise<HandbookPurchaseControls|undefined>;error(status:number,code:string,message:string):Error;companyMember(company:string,user:string):Promise<boolean>;requireServiceOwner(actor:WorkContext):Promise<void>;serviceOwner(actor:WorkContext):Promise<boolean>;page(actor:WorkContext,q:string,editing:boolean,cursor?:string):Promise<ListPage<HandbookListRow>>;};
export function createHandbookService(ports:HandbookPorts){
async function handbookAccess(actor: WorkContext) {
  if (!(await ports.companyMember(actor.companyId, actor.userId)))
    throw ports.error(
      403,
      "MEMBERSHIP_REQUIRED",
      "Current company membership is required.",
    );
  const purchase=await ports.purchaseAccess(actor);
  const owner = await ports.serviceOwner(actor),
    editor =
      owner ||
      !!(await ports.repository.editor(actor.companyId,actor.userId));
  return { owner, editor, ...(purchase?{purchase}:{}) };
}
async function handbookEditorOptions(
  actor: WorkContext,
) {
  await ports.requireServiceOwner(actor);
  await ports.purchaseAccess(actor);
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
return {handbookAccess,handbookEditorOptions,handbookList,handbookArticle};}

