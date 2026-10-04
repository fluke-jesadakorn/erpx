import {createHash,randomUUID} from 'node:crypto';
import {z} from 'zod';
import {holdingBindingBySlug,holdingResources} from '@/server/runtime';
import {assertProductionModuleEnabled} from '@/server/hospitality-rollout';
import {requestPublicOrigin} from '@/server/public-origin';
import {ApiError,errorResponse} from '@/server/http';
import {isTransactionalSqlDatabase} from '@/server/runtime/types';
import {loadPublicMarketingForm,marketingPresentationAssertions} from '@/server/marketing-public-form';
type Context={params:Promise<{holdingSlug:string;formId:string}>};
const bodySchema=z.object({contactName:z.string().trim().min(2).max(160),contactDetail:z.string().trim().min(3).max(2000),brief:z.string().trim().min(3).max(3000),consentAccepted:z.literal(true),source:z.string().trim().min(2).max(120),campaignId:z.string().min(1).optional()});
async function form(context:Context){const {holdingSlug,formId}=await context.params,binding=await holdingBindingBySlug(holdingSlug),{write}=await holdingResources(binding.id),row=await loadPublicMarketingForm(write,formId);await assertProductionModuleEnabled(write,row.company_id,'marketing');return {write,row};}
export async function GET(_request:Request,context:Context){try{const {row}=await form(context);return Response.json({data:{id:row.id,name:row.name,version:row.version,preview:JSON.parse(row.preview_json),presentation:row.presentation,consentRequired:true}},{headers:{'Cache-Control':'no-store'}});}catch(e){return errorResponse(e);}}
export async function POST(request:Request,context:Context){try{
 if(request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the form website.');
 const key=request.headers.get('Idempotency-Key');if(!key||!/^[A-Za-z0-9:_.-]{8,180}$/.test(key))throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');
 const bodyText=await request.text();if(Buffer.byteLength(bodyText)>12000)throw new ApiError(413,'BODY_TOO_LARGE','Form submission is too large.');
 let parsed:unknown;try{parsed=JSON.parse(bodyText);}catch{throw new ApiError(422,'INVALID_JSON','Invalid form submission.');}
 const body=bodySchema.parse(parsed),{write,row}=await form(context);if(!isTransactionalSqlDatabase(write))throw new ApiError(500,'TRANSACTION_REQUIRED','Form storage is unavailable.');
 const requestHash=createHash('sha256').update(JSON.stringify({formId:row.id,formVersion:row.version,body})).digest('hex'),ip=request.headers.get('cf-connecting-ip')??request.headers.get('x-forwarded-for')?.split(',')[0]??'local',rateKey=createHash('sha256').update(`${row.id}:${ip}`).digest('hex');
 const result=await write.transaction(async tx=>{
  const prior=await tx.prepare('SELECT request_hash,candidate_id FROM marketing_public_receipts WHERE key=?').bind(key).first<{request_hash:string;candidate_id:string}>();if(prior){if(prior.request_hash!==requestHash)throw new ApiError(409,'IDEMPOTENCY_CONFLICT','This submission key belongs to a different form.');return {id:prior.candidate_id,status:'pending'};}
  const current=await tx.prepare("SELECT id FROM marketing_assets WHERE id=? AND company_id=? AND version=? AND status='active' FOR SHARE").bind(row.id,row.company_id,row.version).first();if(!current)throw new ApiError(409,'FORM_CHANGED','The approved form changed. Reload before submitting.');
  await tx.batch(marketingPresentationAssertions(tx,key,row.company_id,row.configuration_json));
  const bucket=Math.floor(Date.now()/60000),limit=await tx.prepare('INSERT INTO marketing_public_limits(key,bucket,count) VALUES(?,?,1) ON CONFLICT(key) DO UPDATE SET count=CASE WHEN marketing_public_limits.bucket=excluded.bucket THEN marketing_public_limits.count+1 ELSE 1 END,bucket=excluded.bucket RETURNING count').bind(rateKey,bucket).first<{count:number}>();if(Number(limit?.count)>5)throw new ApiError(429,'FORM_RATE_LIMIT','Too many submissions. Try again later.');
  const branch=await tx.prepare("SELECT id FROM organization_nodes WHERE company_id=? AND kind IN ('branch','headquarters') AND status='active' ORDER BY id LIMIT 1").bind(row.company_id).first<{id:string}>();if(!branch)throw new ApiError(409,'FORM_BRANCH_REQUIRED','The form has no active receiving branch.');
  if(body.campaignId&&!await tx.prepare('SELECT 1 FROM marketing_campaigns WHERE id=? AND company_id=?').bind(body.campaignId,row.company_id).first())throw new ApiError(422,'CAMPAIGN_UNKNOWN','Campaign ID is unavailable.');
  const now=Date.now(),id=`marketing-candidate-${randomUUID()}`,attribution={campaignId:body.campaignId??'',source:body.source,capture:'public_form',formId:row.id,formVersion:row.version};
  await tx.prepare("INSERT INTO marketing_lead_candidates(id,company_id,branch_id,department_id,form_id,form_version,contact_name,contact_detail,consent_evidence,attribution_json,brief,status,crm_lead_id,created_by,reviewed_by,version,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,'pending',NULL,'public-form',NULL,1,?,?)").bind(id,row.company_id,branch.id,null,row.id,row.version,body.contactName,body.contactDetail,`Explicit form consent at ${new Date(now).toISOString()}`,JSON.stringify(attribution),body.brief,now,now).run();
  await tx.prepare("INSERT INTO marketing_engagement_events(id,candidate_id,version,event_type,channel,evidence,actor_id,created_at) VALUES(?,?,1,'form_submit','web',?,'public-form',?)").bind(`marketing-engagement-${id}`,id,`Form ${row.id} version ${row.version}; consent accepted`,now).run();
  await tx.prepare('INSERT INTO marketing_public_receipts(key,request_hash,candidate_id,created_at) VALUES(?,?,?,?)').bind(key,requestHash,id,now).run();
  await tx.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key).run();
  return {id,status:'pending'};
 });
 return Response.json({data:result},{status:201,headers:{'Cache-Control':'no-store'}});
 }catch(e){return errorResponse(e);}}
