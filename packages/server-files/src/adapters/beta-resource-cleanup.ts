import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {ExpiredUpload} from '@erpx/server-manage_erpx/public/resource-cleanup-contract';
export function betaUploadCleanup(db:SqlDatabase){return {
 async expiredUploads(before:number,limit:number){return (await db.prepare("SELECT id,revision FROM upload_intents WHERE status IN ('pending','uploaded') AND expires_at<? ORDER BY expires_at LIMIT ?").bind(before,limit).all<ExpiredUpload>()).results;},
 async uploadHasAttachment(id:string){return !!await db.prepare('SELECT 1 FROM attachments WHERE upload_id=?').bind(id).first();},
 async expireUpload(id:string,before:number,now:number){await db.prepare("UPDATE upload_intents SET status='expired',updated_at=? WHERE id=? AND status IN ('pending','uploaded') AND expires_at<?").bind(now,id,before).run();},
};}
