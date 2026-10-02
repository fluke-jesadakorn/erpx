import "server-only";
import { createHash } from "node:crypto";
import type { EventMessage } from "../job-projector";
import { isTransactionalSqlDatabase, type SqlDatabase } from "../runtime/types";
import { decodePmsSource, type PmsSource } from "./source";
import { pmsSourcesSql,pmsSourceReadableSql } from "./source-policy";

const events:Record<string,{label:string;group:string}>= {
  "pms.reservation.create":{label:"ยืนยันการจอง / Reservation confirmed",group:"reservation"},
  "pms.reservation.cancel":{label:"ยกเลิกการจอง / Reservation cancelled",group:"reservation"},
  "pms.stay.check_in":{label:"เช็กอิน / Check-in",group:"stay"},
  "pms.stay.check_out":{label:"เช็กเอาต์ / Check-out",group:"stay"},
  "pms.task.complete":{label:"ทำความสะอาดเสร็จ / Room prepared",group:"room"},
  "pms.task.inspect":{label:"ตรวจห้องแล้ว / Room inspected",group:"room"},
  "pms.payment.request_transfer":{label:"เงินโอนรอตรวจ / Bank transfer awaiting Finance",group:"transfer"},
  "pms.payment.verify_transfer":{label:"Finance ยืนยันเงินโอน / Bank transfer verified",group:"transfer"},
  "pms.payment.cancel_transfer":{label:"ยกเลิกการตรวจเงินโอน / Transfer request cancelled",group:"transfer"},
  "pms.charge.correction.request":{label:"ขอตรวจแก้ค่าบริการ / Charge correction requested",group:"correction"},
  "pms.charge.correction.review":{label:"ตรวจคำขอแก้ค่าบริการ / Charge correction reviewed",group:"correction"},
  "pms.refund.request":{label:"ขอคืนเงิน / Refund requested",group:"refund"},
  "pms.refund.approve":{label:"อนุมัติคืนเงิน / Refund approved",group:"refund"},
  "pms.refund.reject":{label:"ปฏิเสธคืนเงิน / Refund rejected",group:"refund"},
  "pms.refund.pay":{label:"จ่ายเงินคืนแล้ว / Refund paid",group:"refund"},
  "pms.shift.close":{label:"ปิดกะแคชเชียร์ / Cashier shift closed",group:"shift"},
  "pms.shift.review":{label:"ตรวจส่วนต่างกะแล้ว / Cash variance reviewed",group:"shift"},
  "pms.night.step":{label:"ผลปิดวัน / Night Audit progress",group:"night"},
};

/** The existing durable outbox calls this adapter. It creates no parallel feed store. */
export async function projectPmsSocialEvent(db:SqlDatabase,message:EventMessage) {
  const event=events[message.eventType];
  if(!event)return;
  if(!isTransactionalSqlDatabase(db))throw new Error("PMS_SOCIAL_TRANSACTION_REQUIRED");
  await db.transaction(async tx=>{
    const original=await tx.prepare(`SELECT e.command_id,e.created_at,c.actor_id,c.result_json
      FROM outbox_events e JOIN command_receipts c ON c.command_id=e.command_id
      WHERE e.id=? AND e.event_type=?`).bind(message.id,message.eventType)
      .first<{command_id:string;created_at:number;actor_id:string;result_json:string}>();
    if(!original)return;
    const result=JSON.parse(original.result_json) as {id?:string;propertyId?:string};
    // Payout commands return their journal entry; the shared work source is the
    // approved refund request referenced by that entry.
    const sourceId=message.eventType==="pms.refund.pay"
      ?await tx.prepare("SELECT data_json::jsonb->>'originalId' id FROM pms_records WHERE id=? AND property_id=? AND kind='entry'")
        .bind(result.id??"",result.propertyId??message.aggregateId).first<string>("id")
      :result.id??message.aggregateId;
    const row=await tx.prepare(`SELECT pr.* FROM (${pmsSourcesSql}) pr
      WHERE pr.id=? AND pr.company_id=? AND pr.property_id=?`)
      .bind(sourceId,message.payload.companyId,result.propertyId??message.aggregateId).first<PmsSource>();
    if(!row)return;
    const source=decodePmsSource(row);
    if(!["reservation","stay","task","transfer","correction","refund","shift","night"].includes(source.kind))return;
    const day=new Date(Number(original.created_at)+7*60*60_000).toISOString().slice(0,10);
    const postId="pms-post-"+createHash("sha256").update(JSON.stringify([source.company_id,source.id,day,event.group])).digest("hex").slice(0,40);
    await tx.prepare("SELECT pg_advisory_xact_lock(hashtext(?))").bind(postId).first();
    if(await tx.prepare("SELECT 1 FROM pms_social_event_receipts WHERE event_id=?").bind(message.id).first())return;
    const body=`${source.property_name} · ${event.label}\n${source.data.state??"updated"} · วันที่ธุรกิจ / Business date ${source.data.date??source.business_date}`;
    await tx.batch([
      tx.prepare(`INSERT INTO social_posts(id,kind,company_id,branch_id,author_id,author_name,body,audience_kind,purpose,last_command_id,created_at,updated_at)
        VALUES(?,'system',?,?,?,'PMS',?,'branch','update',?,?,?) ON CONFLICT(id) DO UPDATE SET
        body=excluded.body,last_command_id=excluded.last_command_id,version=social_posts.version+1,updated_at=excluded.updated_at
        WHERE social_posts.updated_at<=excluded.updated_at`)
        .bind(postId,source.company_id,source.branch_id,original.actor_id,body,original.command_id,Number(original.created_at),Number(original.created_at)),
      tx.prepare("INSERT INTO social_post_sources(post_id,ordinal,source_kind,source_id) VALUES(?,0,'pms_record',?) ON CONFLICT DO NOTHING").bind(postId,source.id),
      tx.prepare("INSERT INTO pms_social_event_receipts(event_id,post_id,created_at) VALUES(?,?,?)").bind(message.id,postId,Date.now()),
    ]);
    const review=message.eventType==="pms.payment.request_transfer"?"finance.approve"
      :["pms.refund.request","pms.charge.correction.request"].includes(message.eventType)?"pms.approve"
      :message.eventType==="pms.task.complete"?"pms.inspect":null;
    const recipient=source.data.recordedBy??source.data.requestedBy??source.data.assigneeId??source.data.userId??null;
    const recipients=(await tx.prepare(`SELECT DISTINCT a.user_id FROM assignments a
      JOIN (${pmsSourcesSql}) pr ON pr.id=? AND pr.company_id=a.company_id
      WHERE a.company_id=? AND ${pmsSourceReadableSql("pr","a.user_id","a.id")}
      AND NOT iam_same_person(a.user_id,?,a.company_id)
      AND ((?::text IS NOT NULL AND iam_role_allows(a.role,a.company_id,?)) OR (?::text IS NULL AND iam_same_person(a.user_id,?,a.company_id)))`)
      .bind(source.id,source.company_id,original.actor_id,review,review,review,recipient).all<{user_id:string}>()).results;
    const version=await tx.prepare("SELECT version FROM social_posts WHERE id=?").bind(postId).first<number>("version");
    for(const recipient of recipients){
      const notificationId="pms-notice-"+createHash("sha256").update(JSON.stringify([postId,recipient.user_id])).digest("hex").slice(0,40);
      await tx.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
        VALUES(?,?,?,'post',?,?,?,?) ON CONFLICT(id) DO UPDATE SET record_version=excluded.record_version,message=excluded.message,
        created_at=excluded.created_at,read_at=NULL WHERE staff_notifications.created_at<=excluded.created_at`)
        .bind(notificationId,source.company_id,recipient.user_id,postId,version,body,Number(original.created_at)).run();
    }
  });
}
