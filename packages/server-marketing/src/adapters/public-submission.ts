import { nativeMarketingStatement } from '@erpx/feature-marketing/adapters/server/native-marketing-command';
import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import {ApiError} from '@erpx/server-runtime/public/http';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {BusinessRuleError} from '@erpx/server-shared/index';
import {publicMarketingReceivingBranch} from '@erpx/server-organization/public/public-marketing-receiving-branch';
import {clearWorkflowCommandAssertions} from '@erpx/server-workflow/public/command-assertion-effects';
import {marketingPresentationAssertions} from '@erpx/server-marketing/adapters/public-form';
import type {PublicMarketingForm} from '@erpx/server-marketing/application/public-form';
import type {PublicMarketingSubmission} from '@erpx/server-marketing/domain/public-submission';
import {createPublicMarketingSubmissionApplication,type PublicMarketingSubmissionUnit} from '@erpx/server-marketing/application/public-submission';
import {withMarketingPublicSubmission} from '@erpx/server-marketing/adapters/purchases';
import {marketingCampaignExistsSql} from './campaign-existence';
function unit(db:SqlDatabase):PublicMarketingSubmissionUnit{return {
 receipt:key=>db.prepare('SELECT request_hash,candidate_id FROM marketing_public_receipts WHERE key=?').bind(key).first<{request_hash:string;candidate_id:string}>(),
 current:row=>db.prepare("SELECT id FROM marketing_assets WHERE id=? AND company_id=? AND version=? AND status='active' FOR SHARE").bind(row.id,row.company_id,row.version).first(),
 presentation:(key,row)=>db.batch(marketingPresentationAssertions(db,key,row.company_id,row.configuration_json)),
 limit:(rateKey,bucket)=>nativeMarketingStatement('marketing_stmt_1', { companyId: '', now: Date.now(), actor: undefined }).bind(rateKey,bucket).first<{count:number}>(),
 branch:companyId=>publicMarketingReceivingBranch(db,companyId),
 campaign:(id,companyId)=>db.prepare(marketingCampaignExistsSql).bind(id,companyId).first(),
 candidate:(id,row,body,branchId,attribution,now)=>nativeMarketingStatement('marketing_stmt_2', { companyId: row.company_id, now: now, actor: undefined }).bind(id,row.company_id,branchId,null,row.id,row.version,body.contactName,body.contactDetail,`Explicit form consent at ${new Date(now).toISOString()}`,attribution,body.brief,now,now).run(),
 engagement:(id,row,now)=>nativeMarketingStatement('marketing_stmt_3', { companyId: '', now: now, actor: undefined }).bind(`marketing-engagement-${id}`,id,`Form ${row.id} version ${row.version}; consent accepted`,now).run(),
 retainReceipt:(key,requestHash,id,now)=>nativeMarketingStatement('marketing_stmt_4', { companyId: '', now: now, actor: undefined }).bind(key,requestHash,id,now).run(),
 clearAssertions:key=>clearWorkflowCommandAssertions(db,key).run()
};}
async function submit(db:SqlDatabase,row:PublicMarketingForm,body:PublicMarketingSubmission,key:string,ip:string,versioned:boolean){
 if(!isTransactionalSqlDatabase(db))throw new ApiError(500,'TRANSACTION_REQUIRED','Form storage is unavailable.');
 const app=createPublicMarketingSubmissionApplication({clock:{now:Date.now},identifiers:{next:randomUUID},hash:value=>createHash('sha256').update(value).digest('hex'),transaction:run=>withMarketingPublicSubmission(db,row,key,async(tx,replay)=>{const result=await run(unit(tx));if(!replay){await tx.batch(marketingPresentationAssertions(tx,key,row.company_id,row.configuration_json));await clearWorkflowCommandAssertions(tx,key).run();}return result;})});
 try{return await (versioned?app.submitVersioned(row,body,key,ip):app.submit(row,body,key,ip));}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}
}
export function submitPublicMarketingForm(db:SqlDatabase,row:PublicMarketingForm,body:PublicMarketingSubmission,key:string,ip:string){return submit(db,row,body,key,ip,false);}
export function submitVersionedPublicMarketingForm(db:SqlDatabase,row:PublicMarketingForm,body:PublicMarketingSubmission,key:string,ip:string){return submit(db,row,body,key,ip,true);}
