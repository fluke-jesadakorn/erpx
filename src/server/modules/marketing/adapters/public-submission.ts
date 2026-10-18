import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import {ApiError} from '../../runtime/public/http';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../runtime/public/runtime/types';
import {BusinessRuleError} from '../../shared';
import {publicMarketingReceivingBranch} from '../../organization/public/public-marketing-receiving-branch';
import {clearWorkflowCommandAssertions} from '../../workflow/public/command-assertion-effects';
import {marketingPresentationAssertions} from './public-form';
import type {PublicMarketingForm} from '../application/public-form';
import type {PublicMarketingSubmission} from '../domain/public-submission';
import {createPublicMarketingSubmissionApplication,type PublicMarketingSubmissionUnit} from '../application/public-submission';
import {withMarketingPublicSubmission} from './purchases';
function unit(db:SqlDatabase):PublicMarketingSubmissionUnit{return {
 receipt:key=>db.prepare('SELECT request_hash,candidate_id FROM marketing_public_receipts WHERE key=?').bind(key).first<{request_hash:string;candidate_id:string}>(),
 current:row=>db.prepare("SELECT id FROM marketing_assets WHERE id=? AND company_id=? AND version=? AND status='active' FOR SHARE").bind(row.id,row.company_id,row.version).first(),
 presentation:(key,row)=>db.batch(marketingPresentationAssertions(db,key,row.company_id,row.configuration_json)),
 limit:(rateKey,bucket)=>db.prepare('INSERT INTO marketing_public_limits(key,bucket,count) VALUES(?,?,1) ON CONFLICT(key) DO UPDATE SET count=CASE WHEN marketing_public_limits.bucket=excluded.bucket THEN marketing_public_limits.count+1 ELSE 1 END,bucket=excluded.bucket RETURNING count').bind(rateKey,bucket).first<{count:number}>(),
 branch:companyId=>publicMarketingReceivingBranch(db,companyId),
 campaign:(id,companyId)=>db.prepare('SELECT 1 FROM marketing_campaigns WHERE id=? AND company_id=?').bind(id,companyId).first(),
 candidate:(id,row,body,branchId,attribution,now)=>db.prepare("INSERT INTO marketing_lead_candidates(id,company_id,branch_id,department_id,form_id,form_version,contact_name,contact_detail,consent_evidence,attribution_json,brief,status,crm_lead_id,created_by,reviewed_by,version,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,'pending',NULL,'public-form',NULL,1,?,?)").bind(id,row.company_id,branchId,null,row.id,row.version,body.contactName,body.contactDetail,`Explicit form consent at ${new Date(now).toISOString()}`,attribution,body.brief,now,now).run(),
 engagement:(id,row,now)=>db.prepare("INSERT INTO marketing_engagement_events(id,candidate_id,version,event_type,channel,evidence,actor_id,created_at) VALUES(?,?,1,'form_submit','web',?,'public-form',?)").bind(`marketing-engagement-${id}`,id,`Form ${row.id} version ${row.version}; consent accepted`,now).run(),
 retainReceipt:(key,requestHash,id,now)=>db.prepare('INSERT INTO marketing_public_receipts(key,request_hash,candidate_id,created_at) VALUES(?,?,?,?)').bind(key,requestHash,id,now).run(),
 clearAssertions:key=>clearWorkflowCommandAssertions(db,key).run()
};}
async function submit(db:SqlDatabase,row:PublicMarketingForm,body:PublicMarketingSubmission,key:string,ip:string,versioned:boolean){
 if(!isTransactionalSqlDatabase(db))throw new ApiError(500,'TRANSACTION_REQUIRED','Form storage is unavailable.');
 const app=createPublicMarketingSubmissionApplication({clock:{now:Date.now},identifiers:{next:randomUUID},hash:value=>createHash('sha256').update(value).digest('hex'),transaction:run=>withMarketingPublicSubmission(db,row,key,async(tx,replay)=>{const result=await run(unit(tx));if(!replay){await tx.batch(marketingPresentationAssertions(tx,key,row.company_id,row.configuration_json));await clearWorkflowCommandAssertions(tx,key).run();}return result;})});
 try{return await (versioned?app.submitVersioned(row,body,key,ip):app.submit(row,body,key,ip));}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}
}
export function submitPublicMarketingForm(db:SqlDatabase,row:PublicMarketingForm,body:PublicMarketingSubmission,key:string,ip:string){return submit(db,row,body,key,ip,false);}
export function submitVersionedPublicMarketingForm(db:SqlDatabase,row:PublicMarketingForm,body:PublicMarketingSubmission,key:string,ip:string){return submit(db,row,body,key,ip,true);}
