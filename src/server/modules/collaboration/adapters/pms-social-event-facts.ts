import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { PmsSocialPost, PmsSocialNotice } from "../../pms/public/social-event-contracts";
export function pmsSocialWrites(db: SqlDatabase) {
    return {
        post: (p: PmsSocialPost) => db.prepare(`INSERT INTO social_posts(id,kind,company_id,branch_id,author_id,author_name,body,audience_kind,purpose,last_command_id,created_at,updated_at)
        VALUES(?,'system',?,?,?,'PMS',?,'branch','update',?,?,?) ON CONFLICT(id) DO UPDATE SET
        body=excluded.body,last_command_id=excluded.last_command_id,version=social_posts.version+1,updated_at=excluded.updated_at
        WHERE social_posts.updated_at<=excluded.updated_at`).bind(p.postId, p.companyId, p.branchId, p.actorId, p.body, p.commandId, p.at, p.at),
        source: (postId: string, id: string) => db.prepare("INSERT INTO social_post_sources(post_id,ordinal,source_kind,source_id) VALUES(?,0,'pms_record',?) ON CONFLICT DO NOTHING").bind(postId, id),
        version: (postId: string) => db.prepare("SELECT version FROM social_posts WHERE id=?").bind(postId).first<number>("version"),
        notify: (n: PmsSocialNotice) => db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
        VALUES(?,?,?,'post',?,?,?,?) ON CONFLICT(id) DO UPDATE SET record_version=excluded.record_version,message=excluded.message,
        created_at=excluded.created_at,read_at=NULL WHERE staff_notifications.created_at<=excluded.created_at`).bind(n.id, n.companyId, n.userId, n.postId, n.version, n.body, n.at).run()
    };
}
