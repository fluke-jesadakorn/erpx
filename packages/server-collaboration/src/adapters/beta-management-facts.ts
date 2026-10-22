import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function betaManagementFacts(db:SqlDatabase){return {async postDrafts(userId:string,companyId:string){return (await db
                .prepare(
                  "SELECT id,body,branch_id,department_id,audience_kind,audience_id,participants_json,sources_json,purpose,version,mentions_json,content_json,rich_content_json FROM social_post_drafts WHERE author_id=? AND company_id=? AND published_post_id IS NULL AND comment_parent_post_id IS NULL AND edit_post_id IS NULL ORDER BY updated_at DESC LIMIT 20",
                )
                .bind(userId, companyId)
                .all()).results;},
async reports(companyId:string){return (await db
                .prepare(
                  "SELECT r.id,r.reason,r.post_id,p.state FROM social_reports r JOIN social_posts p ON p.id=r.post_id WHERE p.company_id=? ORDER BY r.created_at DESC LIMIT 100",
                )
                .bind(companyId)
                .all()).results;},
async postTask(id:string | undefined){return db
            .prepare("SELECT task_id FROM social_task_origins WHERE post_id=?")
            .bind(id)
            .first<{ task_id: string }>();},
async recoverPostTask(id:string | undefined){return db
                .prepare(
                  "SELECT task_id FROM social_task_origins WHERE post_id=?",
                )
                .bind(id)
                .first<{ task_id: string }>();}};}
