import 'server-only';
import {nativeHoldingConfiguration} from '@erpx/native-sdk/holding-target';
import {nativeServiceInvoicePaymentEvidenceRequest} from '@erpx/native-sdk/field-service-parts';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {currentSession} from '@erpx/server-identity_access/public/authorization';
const fail=(code:string,status=503):never=>{throw new ApiError(status,code,'The native invoice evidence admission is unavailable.');};
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
/** A Field invoice cannot authorize supplier evidence. The native refusal is
 * not a transactional source guard or a success path into legacy mutation. */
export async function rejectNativeServiceInvoicePaymentEvidence(actor:WorkContext,id:string):Promise<never>{
 const raw=process.env.ERPX_NATIVE_FIELD_SERVICE_WORKSPACE_CONFIG;if(!raw)return fail('NATIVE_SERVICE_INVOICE_TARGET_UNAVAILABLE');
 let config:ReturnType<typeof nativeHoldingConfiguration>;try{config=nativeHoldingConfiguration(raw,process.env.NODE_ENV==='development'&&process.env.NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE==='container-localhost'?'contributor':'public');}catch{return fail('NATIVE_SERVICE_INVOICE_CONFIGURATION_INVALID');}
 const {requestHeaders:incoming,session}=await currentSession();
 const target=config.holdings.find(h=>h.holdingId===actor.holdingId&&h.holdingSlug===actor.holdingSlug&&new URL(h.origin).host===incoming.get('host'));
 if(!target||!actor.authority||session.user.id!==actor.userId)return fail('WORK_CONTEXT_CHANGED',409);
 const headers=new Headers({host:new URL(target.origin).host,accept:'application/json','x-erpx-context-company':actor.companyId,'x-erpx-context-assignment':actor.assignmentId,'x-erpx-context-assignment-version':String(actor.authority.assignmentVersion),'x-erpx-context-revision':String(actor.authority.revision)});
 const cookie=incoming.get('cookie');if(cookie){if(new TextEncoder().encode(cookie).length>8192)return fail('NATIVE_SERVICE_INVOICE_COOKIE_LIMIT');headers.set('cookie',cookie);}
 if(actor.scope.kind!=='holding'&&actor.scope.branchSlug)headers.set('x-erpx-workspace-branch',actor.scope.branchSlug);
 if(actor.scope.kind==='department')headers.set('x-erpx-workspace-department',actor.scope.departmentSlug);
 let reply:Awaited<ReturnType<typeof nativeServiceInvoicePaymentEvidenceRequest>>;
 try{reply=await nativeServiceInvoicePaymentEvidenceRequest({baseUrl:config.privateBaseUrl,timeoutMs:config.timeoutMs,slug:target.holdingSlug,headers,id});}catch{return fail('NATIVE_SERVICE_INVOICE_UNAVAILABLE');}
 if(reply.status>=400&&reply.status<=599&&object(reply.body)&&object(reply.body.error)&&typeof reply.body.error.code==='string'&&/^[A-Z0-9_]{1,100}$/.test(reply.body.error.code))return fail(reply.body.error.code,reply.status);
 return fail('NATIVE_SERVICE_INVOICE_EVIDENCE_RESPONSE_INVALID',502);
}
