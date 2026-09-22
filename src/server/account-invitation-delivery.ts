import "server-only";
import { createHmac } from "node:crypto";
import type { SqlDatabase } from "./runtime/types";
import { sendBetaEmail } from "./beta-email";
import type { BetaEnvironment } from "./beta-config";

type DeliveryEnv=BetaEnvironment&{BETTER_AUTH_SECRET?:unknown;ERPX_PUBLIC_ORIGIN?:unknown;BETTER_AUTH_URL?:unknown};
export async function promoteAccountInvitationDeliveries(control:SqlDatabase,holding:SqlDatabase,holdingId:string,now=Date.now()){
  const waiting=(await control.prepare("SELECT id,invitation_id FROM account_invitation_delivery_jobs WHERE holding_id=? AND state='awaiting_approval' ORDER BY created_at LIMIT 100").bind(holdingId).all<{id:string;invitation_id:string}>()).results;
  for(const job of waiting){
    const review=await holding.prepare("SELECT status FROM iam_reviews WHERE kind='invitation.authorize' AND payload->>'invitationId'=? ORDER BY created_at DESC LIMIT 1").bind(job.invitation_id).first<{status:string}>();
    if(review?.status==="approved")await control.prepare("UPDATE account_invitation_delivery_jobs SET state='queued',next_attempt_at=?,updated_at=? WHERE id=? AND state='awaiting_approval'").bind(now,now,job.id).run();
    else if(review?.status==="rejected")await control.prepare("UPDATE account_invitation_delivery_jobs SET state='cancelled',error='INVITATION_REJECTED',updated_at=? WHERE id=? AND state='awaiting_approval'").bind(now,job.id).run();
  }
}
export async function deliverAccountInvitation(control:SqlDatabase,env:DeliveryEnv,send:(env:DeliveryEnv,to:string,subject:string,url:string,key?:string)=>Promise<string|null|void>=sendBetaEmail,now=Date.now()){
  await control.prepare("UPDATE account_invitation_delivery_jobs SET state=CASE WHEN attempts<6 THEN 'queued' ELSE 'failed' END,lease_token=NULL,lease_until=NULL,error='DELIVERY_INTERRUPTED',next_attempt_at=?,updated_at=? WHERE state='running' AND lease_until<?").bind(now,now,now).run();
  const lease=crypto.randomUUID(),job=await control.prepare("UPDATE account_invitation_delivery_jobs SET state='running',attempts=attempts+1,lease_token=?,lease_until=?,updated_at=? WHERE id=(SELECT id FROM account_invitation_delivery_jobs WHERE state='queued' AND next_attempt_at<=? ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING id,invitation_id,attempts,lease_token").bind(lease,now+30000,now,now).first<{id:string;invitation_id:string;attempts:number;lease_token:string}>();
  if(!job)return false;
  try{
    const invite=await control.prepare("SELECT email,name,status,expires_at FROM account_invitations WHERE id=?").bind(job.invitation_id).first<{email:string;name:string;status:string;expires_at:number}>();
    if(!invite||invite.status!=="pending"||invite.expires_at<=now){await control.prepare("UPDATE account_invitation_delivery_jobs SET state='cancelled',lease_token=NULL,lease_until=NULL,error='INVITATION_UNAVAILABLE',updated_at=? WHERE id=? AND lease_token=?").bind(now,job.id,lease).run();return true;}
    const secret=String(env.BETTER_AUTH_SECRET??""),origin=String(env.ERPX_PUBLIC_ORIGIN??env.BETTER_AUTH_URL??"");
    if(!secret)throw new Error("INVITATION_SECRET_NOT_CONFIGURED");
    if(!origin)throw new Error("PUBLIC_ORIGIN_NOT_CONFIGURED");
    const token=createHmac("sha256",secret).update(`invitation:${job.invitation_id}`).digest("hex"),url=new URL(`/activate?token=${encodeURIComponent(token)}`,origin).href;
    const providerId=await send(env,invite.email,"Activate your ERPX account",url,job.id);
    await control.prepare("UPDATE account_invitation_delivery_jobs SET state='sent',sent_at=?,provider_id=?,lease_token=NULL,lease_until=NULL,error=NULL,updated_at=? WHERE id=? AND lease_token=?").bind(now,typeof providerId==="string"?providerId:job.id,now,job.id,lease).run();
  }catch(error){
    const code=error instanceof Error&&["EMAIL_NOT_CONFIGURED","PUBLIC_ORIGIN_NOT_CONFIGURED","INVITATION_SECRET_NOT_CONFIGURED"].includes(error.message)?error.message:"EMAIL_DELIVERY_FAILED";
    await control.prepare("UPDATE account_invitation_delivery_jobs SET state=?,error=?,next_attempt_at=?,lease_token=NULL,lease_until=NULL,updated_at=? WHERE id=? AND lease_token=?").bind(job.attempts>=6?"failed":"queued",code,now+Math.min(3600000,30000*2**job.attempts),now,job.id,lease).run();
  }
  return true;
}
