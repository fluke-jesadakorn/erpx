import { readListPage } from "@erpx/server-work_projection/public/list-page";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";
import {
  companyMember,
  requireServiceOwner,
  serviceOwner,
} from "@erpx/server-support/public/service-access";

import {createHandbookService} from "@erpx/server-collaboration/application/handbook";
import type {Article,HandbookRevision} from "@erpx/server-collaboration/domain/handbook";
import {handbookPurchaseAccess,handbookReadScope} from '@erpx/server-collaboration/adapters/handbook-purchases';
export {handbookSchema,handbookActionSchema,handbookEditorSchema} from "@erpx/server-collaboration/domain/handbook";
function bind(db:SqlDatabase){return createHandbookService({purchaseAccess:actor=>handbookPurchaseAccess(db,actor),error:(status,code,message)=>new ApiError(status,code,message),companyMember:(company,user)=>companyMember(db,company,user),requireServiceOwner:actor=>requireServiceOwner(db,actor),serviceOwner:actor=>serviceOwner(db,actor),page:async(actor,q,editing,cursor)=>{const purchased=await handbookReadScope(actor);return readListPage<{id:string;page_key:string;version:number;archived:number;revision:number;title:string;category:string;excerpt:string;published_at:number|null}>(db,{
    sql:`SELECT a.id,a.version,a.archived,r.revision,r.title,r.title AS page_key,r.category,left(r.body,220) AS excerpt,r.published_at
      FROM handbook_articles a JOIN handbook_revisions r ON r.article_id=a.id AND r.company_id=a.company_id
        AND r.revision=CASE WHEN ?=1 THEN a.draft_revision ELSE a.published_revision END
      WHERE a.company_id=? AND (?=1 OR a.archived=0) AND strpos(lower(r.title||' '||r.body||' '||r.category),lower(?))>0 AND ${purchased.where}`,
    args:[editing?1:0,actor.companyId,editing?1:0,q,...purchased.args],cursor,scope:[actor.companyId,actor.userId,'handbook',editing,q],
  });},repository:{async editor(companyId:string,userId:string){return db
        .prepare(
          "SELECT 1 FROM handbook_editors WHERE company_id=? AND user_id=?",
        )
        .bind(companyId, userId)
        .first();},
async settings(companyId:string){return db
          .prepare("SELECT version FROM handbook_settings WHERE company_id=?")
          .bind(companyId)
          .first<{ version: number }>();},
async editors(companyId:string){return db
        .prepare(
          "SELECT user_id,granted_at FROM handbook_editors WHERE company_id=? ORDER BY user_id",
        )
        .bind(companyId)
        .all<{ user_id: string; granted_at: number }>();},
async article(id:string,companyId:string){return db
    .prepare("SELECT * FROM handbook_articles WHERE id=? AND company_id=?")
    .bind(id, companyId)
    .first<Article>();},
async revision(id:string,companyId:string,value:number|null,value2:0 | 1){return db
    .prepare(
      "SELECT * FROM handbook_revisions WHERE article_id=? AND company_id=? AND revision=? AND (?=1 OR published_at IS NOT NULL)",
    )
    .bind(
      id,
      companyId,
      value,
      value2,
    )
    .first<HandbookRevision>();},
async history(id:string,companyId:string,value:0 | 1){return db
      .prepare(
        "SELECT revision,title,category,created_by,created_at,published_by,published_at FROM handbook_revisions WHERE article_id=? AND company_id=? AND (?=1 OR published_at IS NOT NULL) ORDER BY revision DESC LIMIT 100",
      )
      .bind(id, companyId, value)
      .all<Omit<HandbookRevision, "body">>();}}});}
export function handbookAccess(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["handbookAccess"]>){return bind(db).handbookAccess(...args);}
export function handbookEditorOptions(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["handbookEditorOptions"]>){return bind(db).handbookEditorOptions(...args);}
export function handbookList(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["handbookList"]>){return bind(db).handbookList(...args);}
export function handbookArticle(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["handbookArticle"]>){return bind(db).handbookArticle(...args);}
