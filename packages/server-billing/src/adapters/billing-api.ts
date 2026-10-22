import "server-only";
import { createHash } from "node:crypto";
import { requireManageErpxContext, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { runtimeEnv, holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { requireCompanyAuthority } from "@erpx/server-operations/public/staff-policy";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { betaBody } from "@erpx/server-manage_erpx/public/beta-http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import {billingCommand,billingView} from "@erpx/server-billing/adapters/billing";
import { billingStripe } from "@erpx/server-billing/adapters/billing-stripe";
import {readTargetBillingPeriod} from "@erpx/server-billing/adapters/billing-target";

import { attachmentUploadsFrozen, attachmentsV2ReadOnly } from "@erpx/server-files/public/attachments/feature";
import { detectedAttachmentMediaType } from "@erpx/server-files/public/attachments/preview";
import { attachmentScannerReadiness } from "@erpx/server-files/public/attachments/scanner";
import { billingEvidenceStore } from "@erpx/server-billing/adapters/billing-attachments";
import {createBillingApi,type BillingApiRepository} from '@erpx/server-billing/application/api';
import type {RuntimeEnv} from '@erpx/server-runtime/public/runtime/env';
import {billingHoldingFacts} from '@erpx/server-runtime/public/billing-holding-facts';
export function billingHandler(platform:boolean){return (request:Request,context:{params:Promise<{holdingSlug?:string;path?:string[]}>})=>{let env:RuntimeEnv;const database=()=>env.CONTROL_DB as PostgresDatabase;const repository:BillingApiRepository={platformAccounts:()=>billingHoldingFacts(database().pool).platformAccounts(),
holdingSlug:v0=>billingHoldingFacts(database().pool).holdingSlug(v0),
scopedEvidence:(v0,v1)=>database().pool.query("SELECT * FROM billing_evidence WHERE id=$1 AND holding_id=$2",[v0,v1]),
deniedUnavailable:(v0,v1,v2,v3,v4,v5)=>database().pool.query("INSERT INTO billing_evidence_access_events(id,evidence_id,holding_id,actor,outcome,reason,created_at) VALUES($1,$2,$3,$4,'denied',$5,$6)",[v0,v1,v2,v3,v4,v5]),
deniedMissingFile:(v0,v1,v2,v3,v4,v5)=>database().pool.query("INSERT INTO billing_evidence_access_events(id,evidence_id,holding_id,actor,outcome,reason,created_at) VALUES($1,$2,$3,$4,'denied',$5,$6)",[v0,v1,v2,v3,v4,v5]),
allowedEvidence:(v0,v1,v2,v3,v4)=>database().pool.query("INSERT INTO billing_evidence_access_events(id,evidence_id,holding_id,actor,outcome,created_at) VALUES($1,$2,$3,$4,'allowed',$5)",[v0,v1,v2,v3,v4]),
existingEvidence:(v0)=>database().pool.query("SELECT * FROM billing_evidence WHERE id=$1",[v0]),
billingAccountExists:(v0)=>database().pool.query("SELECT 1 FROM billing_accounts WHERE holding_id=$1",[v0]),retainQuarantine:async({id,holdingId,filename,mime,size,sha,actorId,now,objectKey,detected})=>{const client=await database().pool.connect();try{await client.query('BEGIN');await client.query(`INSERT INTO billing_evidence(id,holding_id,filename,mime,size,sha256,actor,created_at,object_key,detected_mime,source_state) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT(id) DO NOTHING`,[id,holdingId,filename,mime,size,sha,actorId,now,objectKey,detected,'quarantined']);await client.query(`INSERT INTO billing_evidence_storage_ledger(id,evidence_id,holding_id,object_key,bytes,state,created_at,updated_at) SELECT $1,id,holding_id,object_key,size,'stored',$2,$2 FROM billing_evidence WHERE id=$3 ON CONFLICT(object_key) DO NOTHING`,[`billing-storage-${id}`,now,id]);await client.query("INSERT INTO billing_evidence_scan_attempts(id,evidence_id,attempt,state,created_at) VALUES($1,$2,1,'pending',$3) ON CONFLICT(evidence_id,attempt) DO NOTHING",[`billing-scan-${id}`,id,now]);await client.query("INSERT INTO billing_evidence_events(id,evidence_id,holding_id,actor,action,details_json,created_at) VALUES($1,$2,$3,$4,'file.quarantined',$5,$6) ON CONFLICT(id) DO NOTHING",[`billing-event-${id}`,id,holdingId,actorId,JSON.stringify({sha256:sha,bytes:size,detectedMime:detected}),now]);await client.query('COMMIT')}catch(error){await client.query('ROLLBACK');throw error}finally{client.release()}}};const run=createBillingApi({repository,ready:async()=>{env=await runtimeEnv()},platformActor:async()=> (await requireManageErpxContext()).platform,workActor:requireWorkContext,companyAuthority:async a=>requireCompanyAuthority((await holdingResources(a.holdingId)).write,a,['holding_owner']),error:(...args)=>new ApiError(...args),errorResponse,now:Date.now,id:()=>crypto.randomUUID(),origin:()=>env.BETTER_AUTH_URL,publicOrigin:requestPublicOrigin,readOnly:attachmentsV2ReadOnly,uploadsFrozen:attachmentUploadsFrozen,scannerReadiness:()=>attachmentScannerReadiness({...process.env,ERPX_ATTACHMENTS_V2:'true'}),object:key=>billingEvidenceStore(env).get(key),objectHead:key=>billingEvidenceStore(env).head(key),putObject:(key,bytes,options)=>billingEvidenceStore(env).put(key,bytes,options),hash:value=>createHash('sha256').update(value).digest('hex'),detect:detectedAttachmentMediaType,target:(id,month)=>readTargetBillingPeriod(database(),id,month),view:id=>billingView(database(),id),body:betaBody,command:(actor,id,input,key,returnUrl)=>billingCommand(database(),actor,id,input,key,billingStripe(env),returnUrl)});return run.billingHandler(platform)(request,context)}}
