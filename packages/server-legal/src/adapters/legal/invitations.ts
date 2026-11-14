import {holdingRoutingEnabled,holdingOrigin} from '@erpx/feature-runtime/lib/holding-hosts';
import {legalQueuedPurchasePredicate} from '@erpx/server-legal/adapters/queued-purchases';
import {publicSurfaceOrigin} from '@erpx/server-identity_access/public/public-surface-entry';
import "server-only";
import {enrollmentToken} from "@erpx/server-legal/adapters/legal/enrollment";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {BetaEnvironment} from "@erpx/server-manage_erpx/public/beta-config";
import {sendBetaEmail} from "@erpx/server-manage_erpx/public/beta-email";
import {legalRuntimePredicate} from "@erpx/server-legal/adapters/legal/runtime-gate";
import {createLegalInvitationWorker,type LegalInvitationJob} from "@erpx/server-legal/application/invitations";
export function drainLegalInvitations(db:SqlDatabase,env:BetaEnvironment&{BETTER_AUTH_URL?:string;BETTER_AUTH_SECRET?:string},send=sendBetaEmail){const portalOrigin=publicSurfaceOrigin('legal-portal',undefined);return createLegalInvitationWorker({clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},publicOrigin:portalOrigin??env.BETTER_AUTH_URL,shareSpecificLink:!!portalOrigin||holdingRoutingEnabled(process.env),documentUrl:slug=>holdingRoutingEnabled(process.env)&&holdingOrigin(slug)?new URL('/legal-portal',holdingOrigin(slug)!).href:null,enrollmentOrigin:publicSurfaceOrigin('account',env.BETTER_AUTH_URL),enrollmentEnabled:Boolean(env.BETTER_AUTH_SECRET),enrollmentToken:(slug,shareId,expiresAt)=>enrollmentToken(env.BETTER_AUTH_SECRET!,slug,shareId,expiresAt),send:async(email,subject,body,key)=>{await send(env,email,subject,body,key,{sensitive:true});},repository:{
recover:async now=>{await db.prepare("UPDATE legal_invitation_jobs SET state=CASE WHEN attempts<6 THEN 'queued' ELSE 'failed' END,lease_token=NULL,lease_until=NULL,error='DELIVERY_INTERRUPTED' WHERE state='running' AND lease_until<?").bind(now).run();},
claim:async(token,now)=>db.prepare(`UPDATE legal_invitation_jobs SET state='running',attempts=attempts+1,lease_token=?,lease_until=? WHERE id=(SELECT j.id FROM legal_invitation_jobs j JOIN legal_shares s ON s.id=j.share_id JOIN legal_items i ON i.id=s.item_id WHERE j.state='queued' AND j.next_attempt_at<=? AND ${legalRuntimePredicate('i.company_id','invitations')} AND ${await legalQueuedPurchasePredicate(db,'i','invitations')} ORDER BY j.created_at FOR UPDATE OF j SKIP LOCKED LIMIT 1) RETURNING *`).bind(token,now+30000,now).first<LegalInvitationJob>(),
share:async(id,now)=>db.prepare(`SELECT s.email,s.expires_at FROM legal_shares s JOIN legal_items i ON i.id=s.item_id JOIN legal_settings c ON c.company_id=i.company_id AND c.enabled WHERE s.id=? AND s.revoked_at IS NULL AND s.expires_at>? AND ${legalRuntimePredicate('i.company_id','invitations')} AND ${await legalQueuedPurchasePredicate(db,'i','invitations')}`).bind(id,now).first<{email:string;expires_at:number}>(),
cancel:async(id,token)=>{await db.prepare("UPDATE legal_invitation_jobs SET state='cancelled',lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?").bind(id,token).run();},
sent:async(id,token,now)=>{await db.prepare("UPDATE legal_invitation_jobs SET state='sent',sent_at=?,lease_token=NULL,lease_until=NULL,error=NULL WHERE id=? AND lease_token=?").bind(now,id,token).run();},
failed:async(id,token,state,error,nextAttempt)=>{await db.prepare("UPDATE legal_invitation_jobs SET state=?,error=?,next_attempt_at=?,lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?").bind(state,error,nextAttempt,id,token).run();}
}})();}
