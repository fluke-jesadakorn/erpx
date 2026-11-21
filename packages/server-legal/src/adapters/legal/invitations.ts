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
import {legalWave3Statement} from "../native-legal-wave3";
export function drainLegalInvitations(db:SqlDatabase,env:BetaEnvironment&{BETTER_AUTH_URL?:string;BETTER_AUTH_SECRET?:string},send=sendBetaEmail){const portalOrigin=publicSurfaceOrigin('legal-portal',undefined);return createLegalInvitationWorker({clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},publicOrigin:portalOrigin??env.BETTER_AUTH_URL,shareSpecificLink:!!portalOrigin||holdingRoutingEnabled(process.env),documentUrl:slug=>holdingRoutingEnabled(process.env)&&holdingOrigin(slug)?new URL('/legal-portal',holdingOrigin(slug)!).href:null,enrollmentOrigin:publicSurfaceOrigin('account',env.BETTER_AUTH_URL),enrollmentEnabled:Boolean(env.BETTER_AUTH_SECRET),enrollmentToken:(slug,shareId,expiresAt)=>enrollmentToken(env.BETTER_AUTH_SECRET!,slug,shareId,expiresAt),send:async(email,subject,body,key)=>{await send(env,email,subject,body,key,{sensitive:true});},repository:{
recover:async now=>{await (await legalWave3Statement(db,{kind:'system'},'legal_invite_recover',[now],`legal-invite-recover:${now}`,now)).run();},
claim:async(token,now)=>(await legalWave3Statement(db,{kind:'system'},'legal_invite_claim',[token,now+30000,now,`${legalRuntimePredicate('i.company_id','invitations')} AND ${await legalQueuedPurchasePredicate(db,'i','invitations')}`],`legal-invite-claim:${token}`,now)).first<LegalInvitationJob>(),
share:async(id,now)=>db.prepare(`SELECT s.email,s.expires_at FROM legal_shares s JOIN legal_items i ON i.id=s.item_id JOIN legal_settings c ON c.company_id=i.company_id AND c.enabled WHERE s.id=? AND s.revoked_at IS NULL AND s.expires_at>? AND ${legalRuntimePredicate('i.company_id','invitations')} AND ${await legalQueuedPurchasePredicate(db,'i','invitations')}`).bind(id,now).first<{email:string;expires_at:number}>(),
cancel:async(id,token)=>{await (await legalWave3Statement(db,{kind:'system'},'legal_invite_cancel',[id,token],`legal-invite-cancel:${id}`,Date.now())).run();},
sent:async(id,token,now)=>{await (await legalWave3Statement(db,{kind:'system'},'legal_invite_sent',[now,id,token],`legal-invite-sent:${id}`,now)).run();},
failed:async(id,token,state,error,nextAttempt)=>{await (await legalWave3Statement(db,{kind:'system'},'legal_invite_fail',[state,error,nextAttempt,id,token],`legal-invite-fail:${id}`,Date.now())).run();}
}})();}
