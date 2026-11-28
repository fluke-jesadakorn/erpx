import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/platform-runtime-server/public/runtime/types";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {BusinessRuleError} from "@erpx/platform-shared-server/index";
import {marketingPresentationReferences as references} from "@erpx/marketing-server/domain/presentation";
import {createMarketingPresentationApplication} from "@erpx/marketing-server/application/public-form";
import {prepareMarketingCreativeGuards,approvedMarketingCreative} from "@erpx/files-server/public/marketing-creative";
import {commercePackageEnforcementMode,checkPurchasedCapability} from "@erpx/commercial-server/public/purchase-command-boundary";
import {publicSubmissionReceiptWitness} from "@erpx/marketing-server/adapters/crm-purchase-sources";
import { assertProductionModuleEnabled } from "@erpx/pms-server/public/feature-availability";
import {withPublicMarketingFormRead} from '@erpx/marketing-server/adapters/public-form-read';
import {marketingFormContentSql} from './public-form-content-source';
import {MARKETING_RD_FORM_LOCK,nativeMarketingPlan} from './native-marketing-read';
import {readNativePublicFormSource} from './native-public-form-source';
import { LANE40_FORM_CONTENT_ROW, readMarketingLane40ForCompany } from './native-lane-40/client';
function translate(error:unknown):never{if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}
export function marketingPresentationReferences(configuration:unknown){try{return references(configuration);}catch(error){return translate(error);}}
const contentPredicate=`EXISTS(${marketingFormContentSql('1')})`;
export async function marketingPresentationAssertions(db:SqlDatabase,key:string,companyId:string,configuration:unknown):Promise<SqlStatement[]>{
 const refs=marketingPresentationReferences(configuration),steps:SqlStatement[]=[];
 if(refs.contentAssetId){const lock=await nativeMarketingPlan(MARKETING_RD_FORM_LOCK);steps.push(db.prepare(`${lock.sql} FOR SHARE`).bind(refs.contentAssetId,companyId),assertion(db,key,'form-content',contentPredicate,[refs.contentAssetId,companyId,refs.contentVersion!]));}
 if(refs.mediaPublicationId)steps.push(...prepareMarketingCreativeGuards(db,key,companyId,refs.mediaPublicationId,refs.contentAssetId!));return steps;
}
function application(db:SqlDatabase,holdingSlug:string){return createMarketingPresentationApplication({
async content(companyId,refs){
  const rows=await readMarketingLane40ForCompany(LANE40_FORM_CONTENT_ROW,companyId,{assetId:refs.contentAssetId!,version:refs.contentVersion!});
  const row=rows[0];
  return row&&typeof row.id==='string'&&typeof row.version==='number'&&typeof row.preview_json==='string'
    ?{id:row.id,version:row.version,preview_json:row.preview_json}:null;},
media:(companyId,refs)=>approvedMarketingCreative(db,companyId,refs.mediaPublicationId!,refs.contentAssetId!),
async form(id){
  return readNativePublicFormSource(holdingSlug,id);},
});}
export async function marketingFormReadiness(db:SqlDatabase,holdingSlug:string,companyId:string,configuration:unknown){try{return await application(db,holdingSlug).readiness(companyId,configuration);}catch(error){return translate(error);}}
async function load(db:SqlDatabase,holdingSlug:string,id:string,submissionKey?:string){
 const database=db;
 const mode=await commercePackageEnforcementMode(),form=await application(database,holdingSlug).load(id);
 const retained=mode!=='disabled'&&submissionKey?await database.prepare(publicSubmissionReceiptWitness).bind(submissionKey,form.company_id,form.id,form.version).first():null;
 await checkPurchasedCapability(database,mode,{companyId:form.company_id,capability:'sales.marketing',purpose:retained?'history':'new-work'});await assertProductionModuleEnabled(database,form.company_id,'marketing');return form;
}
export async function loadPublicMarketingForm(db:SqlDatabase,holdingSlug:string,id:string,submissionKey?:string){try{return await withPublicMarketingFormRead(db,id,submissionKey,database=>load(database,holdingSlug,id,submissionKey));}catch(error){return translate(error);}}
/** Submission preparation is not a public presentation response. The existing
 * command takes its own complete source leases and validates this captured
 * revision before consent/effects; retain its original stale-submit outcome. */
export async function loadPublicMarketingSubmissionForm(db:SqlDatabase,holdingSlug:string,id:string,submissionKey:string){try{return await load(db,holdingSlug,id,submissionKey);}catch(error){return translate(error);}}
