import { readListPage } from "@/server/modules/work_projection/public/list-page";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import "server-only";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { ApiError } from "../../runtime/public/http";
import {
  companyMember,
  requireServiceOwner,
  serviceAuthoritySnapshot,
  serviceOwner,
} from "@/server/modules/support/public/service-access";
import { authorityGuard } from "@/server/modules/operations/public/staff-policy";

import type {SqlStatement} from "../../runtime/public/runtime/types";
import {createHandbookService} from "../application/handbook";
import type {Article,HandbookRevision} from "../domain/handbook";
export {handbookSchema,handbookActionSchema,handbookEditorSchema} from "../domain/handbook";
function bind(db:SqlDatabase){return createHandbookService<SqlStatement>({error:(status,code,message)=>new ApiError(status,code,message),now:Date.now,companyMember:(company,user)=>companyMember(db,company,user),requireServiceOwner:actor=>requireServiceOwner(db,actor),serviceOwner:actor=>serviceOwner(db,actor),serviceAuthoritySnapshot:company=>serviceAuthoritySnapshot(db,company),authorityGuard:(key,actor,roles)=>authorityGuard(db,key,actor,roles),page:(actor,q,editing,cursor)=>readListPage<{id:string;page_key:string;version:number;archived:number;revision:number;title:string;category:string;excerpt:string;published_at:number|null}>(db,{
    sql:`SELECT a.id,a.version,a.archived,r.revision,r.title,r.title AS page_key,r.category,left(r.body,220) AS excerpt,r.published_at
      FROM handbook_articles a JOIN handbook_revisions r ON r.article_id=a.id AND r.company_id=a.company_id
        AND r.revision=CASE WHEN ?=1 THEN a.draft_revision ELSE a.published_revision END
      WHERE a.company_id=? AND (?=1 OR a.archived=0) AND strpos(lower(r.title||' '||r.body||' '||r.category),lower(?))>0`,
    args:[editing?1:0,actor.companyId,editing?1:0,q],cursor,scope:[actor.companyId,actor.userId,'handbook',editing,q],
  }),commands:{replay:(actor,key,operation,id,input)=>replayCommand(db,actor,key,operation,id,input),commit:(actor,key,operation,input,id,version,result,effects,scope)=>executeCommand(db,actor,key,operation,input,id,version,result,effects,scope)},repository:{async editor(companyId:string,userId:string){return db
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
editorVersion(key:string,value:"editor-version",companyId:string,expectedVersion:number){return assertion(
        db,
        key,
        value,
        "COALESCE((SELECT version FROM handbook_settings WHERE company_id=?),0)=?",
        [companyId, expectedVersion],
      );},
saveSettings(companyId:string,version:number){return db
        .prepare(
          "INSERT INTO handbook_settings(company_id,version) VALUES(?,?) ON CONFLICT(company_id) DO UPDATE SET version=excluded.version",
        )
        .bind(companyId, version);},
grantEditor(companyId:string,userId:string,userId2:string,now:number){return db
            .prepare(
              "INSERT INTO handbook_editors(company_id,user_id,granted_by,granted_at) VALUES(?,?,?,?) ON CONFLICT(company_id,user_id) DO UPDATE SET granted_by=excluded.granted_by,granted_at=excluded.granted_at",
            )
            .bind(companyId, userId, userId2, now);},
removeEditor(companyId:string,userId:string){return db
            .prepare(
              "DELETE FROM handbook_editors WHERE company_id=? AND user_id=?",
            )
            .bind(companyId, userId);},
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
      .all<Omit<HandbookRevision, "body">>();},
async currentArticle(id:string,companyId:string){return db
        .prepare("SELECT * FROM handbook_articles WHERE id=? AND company_id=?")
        .bind(id, companyId)
        .first<Article>();},
articleVersion(key:string,value:"article-version",recordId:string,companyId:string,expectedVersion:number){return assertion(
              db,
              key,
              value,
              "EXISTS(SELECT 1 FROM handbook_articles WHERE id=? AND company_id=? AND version=?)",
              [recordId, companyId, expectedVersion],
            );},
updateArticle(revision:number,version:number,now:number,recordId:string,companyId:string){return db
              .prepare(
                "UPDATE handbook_articles SET draft_revision=?,version=?,updated_at=? WHERE id=? AND company_id=?",
              )
              .bind(revision, version, now, recordId, companyId);},
createArticle(recordId:string,companyId:string,version:number,revision:number,userId:string,now:number,now2:number){return db
              .prepare(
                "INSERT INTO handbook_articles(id,company_id,version,draft_revision,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?)",
              )
              .bind(
                recordId,
                companyId,
                version,
                revision,
                userId,
                now,
                now2,
              );},
createRevision(companyId:string,recordId:string,revision:number,title:string,category:"policy" | "procedure" | "faq" | "service_guide",body:string,userId:string,now:number){return db
        .prepare(
          "INSERT INTO handbook_revisions(company_id,article_id,revision,title,category,body,created_by,created_at) VALUES(?,?,?,?,?,?,?,?)",
        )
        .bind(
          companyId,
          recordId,
          revision,
          title,
          category,
          body,
          userId,
          now,
        );},
async actionArticle(id:string,companyId:string){return db
    .prepare("SELECT * FROM handbook_articles WHERE id=? AND company_id=?")
    .bind(id, companyId)
    .first<Article>();},
actionVersion(key:string,value:"article-version",id:string,companyId:string,version:number){return assertion(
        db,
        key,
        value,
        "EXISTS(SELECT 1 FROM handbook_articles WHERE id=? AND company_id=? AND version=?)",
        [id, companyId, version],
      );},
async publishDraft(id:string,companyId:string,draft_revision:number){return db
      .prepare(
        "SELECT title,body FROM handbook_revisions WHERE article_id=? AND company_id=? AND revision=?",
      )
      .bind(id, companyId, draft_revision)
      .first<{ title: string; body: string }>();},
publishRevision(userId:string,now:number,id:string,draft_revision:number,companyId:string){return db
        .prepare(
          "UPDATE handbook_revisions SET published_by=COALESCE(published_by,?),published_at=COALESCE(published_at,?) WHERE article_id=? AND revision=? AND company_id=?",
        )
        .bind(userId, now, id, draft_revision, companyId);},
publishArticle(version:number,now:number,id:string,companyId:string){return db
        .prepare(
          "UPDATE handbook_articles SET published_revision=draft_revision,archived=0,version=?,updated_at=? WHERE id=? AND company_id=?",
        )
        .bind(version, now, id, companyId);},
archiveArticle(value:0 | 1,version:number,now:number,id:string,companyId:string){return db
        .prepare(
          "UPDATE handbook_articles SET archived=?,version=?,updated_at=? WHERE id=? AND company_id=?",
        )
        .bind(
          value,
          version,
          now,
          id,
          companyId,
        );}}});}
export function handbookAccess(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["handbookAccess"]>){return bind(db).handbookAccess(...args);}
export function handbookEditorOptions(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["handbookEditorOptions"]>){return bind(db).handbookEditorOptions(...args);}
export function setHandbookEditor(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["setHandbookEditor"]>){return bind(db).setHandbookEditor(...args);}
export function handbookList(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["handbookList"]>){return bind(db).handbookList(...args);}
export function handbookArticle(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["handbookArticle"]>){return bind(db).handbookArticle(...args);}
export function saveHandbookArticle(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["saveHandbookArticle"]>){return bind(db).saveHandbookArticle(...args);}
export function actOnHandbook(db:SqlDatabase,...args:Parameters<ReturnType<typeof bind>["actOnHandbook"]>){return bind(db).actOnHandbook(...args);}
