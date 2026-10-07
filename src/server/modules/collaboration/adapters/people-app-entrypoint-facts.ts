import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
export function taskSocialOrigin(db:SqlDatabase,taskId:string){return db
    .prepare("SELECT post_id FROM social_task_origins WHERE task_id=?")
    .bind(taskId)
    .first<{ post_id: string }>();}
export function sentNoticeRows(db:SqlDatabase,companyId:string,senderId:string){return db.prepare(`SELECT n.id,n.title,n.recipient_count accepted_count,n.created_at,
        (SELECT count(*)::int FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id
         WHERE e.company_id=n.company_id AND e.event_key='notice:'||n.id) delivered_count
        FROM inbox_notices n WHERE n.company_id=? AND n.sender_user_id=? ORDER BY n.created_at DESC,n.id DESC LIMIT 50`)
        .bind(companyId,senderId).all();}
export function sentNoticeRow(db:SqlDatabase,noticeId:string,companyId:string,senderId:string){return db.prepare("SELECT id,title,body,recipient_count,created_at FROM inbox_notices WHERE id=? AND company_id=? AND sender_user_id=?")
        .bind(noticeId,companyId,senderId).first<Record<string,unknown>>();}
export function sentNoticeDeliveryCount(db:SqlDatabase,companyId:string,eventKey:string){return db.prepare(`SELECT count(*)::int n FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id
        WHERE e.company_id=? AND e.event_key=?`).bind(companyId,eventKey).first<{n:number}>();}
