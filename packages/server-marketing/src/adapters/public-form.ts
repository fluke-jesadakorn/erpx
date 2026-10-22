import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {marketingPresentationReferences as references} from "@erpx/server-marketing/domain/presentation";
import {createMarketingPresentationApplication,type PublicMarketingForm,type MarketingContent} from "@erpx/server-marketing/application/public-form";
import {prepareMarketingCreativeGuards,approvedMarketingCreative} from "@erpx/server-files/public/marketing-creative";
import {commercePackageEnforcementMode,checkPurchasedCapability} from "@erpx/server-commercial/public/purchase-command-boundary";
import {crmMarketingCandidateSources} from "@erpx/server-marketing/adapters/crm-purchase-sources";
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import {withPublicMarketingFormRead} from '@erpx/server-marketing/adapters/public-form-read';
function translate(error:unknown):never{if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}
export function marketingPresentationReferences(configuration:unknown){try{return references(configuration);}catch(error){return translate(error);}}
const contentPredicate="EXISTS(SELECT 1 FROM marketing_assets WHERE id=? AND company_id=? AND kind='content' AND status='active' AND version=?)";
export function marketingPresentationAssertions(db:SqlDatabase,key:string,companyId:string,configuration:unknown):SqlStatement[]{
 const refs=marketingPresentationReferences(configuration),steps:SqlStatement[]=[];
 if(refs.contentAssetId)steps.push(db.prepare('SELECT id FROM marketing_assets WHERE id=? AND company_id=? FOR SHARE').bind(refs.contentAssetId,companyId),assertion(db,key,'form-content',contentPredicate,[refs.contentAssetId,companyId,refs.contentVersion!]));
 if(refs.mediaPublicationId)steps.push(...prepareMarketingCreativeGuards(db,key,companyId,refs.mediaPublicationId,refs.contentAssetId!));return steps;
}
function application(db:SqlDatabase){return createMarketingPresentationApplication({
content:(companyId,refs)=>db.prepare("SELECT id,version,preview_json FROM marketing_assets WHERE id=? AND company_id=? AND kind='content' AND status='active' AND version=?").bind(refs.contentAssetId!,companyId,refs.contentVersion!).first<MarketingContent>(),
media:(companyId,refs)=>approvedMarketingCreative(db,companyId,refs.mediaPublicationId!,refs.contentAssetId!),
form:id=>db.prepare("SELECT id,company_id,name,version,preview_json,configuration_json FROM marketing_assets WHERE id=? AND kind='form' AND status='active'").bind(id).first<PublicMarketingForm>(),
});}
export async function marketingFormReadiness(db:SqlDatabase,companyId:string,configuration:unknown){try{return await application(db).readiness(companyId,configuration);}catch(error){return translate(error);}}
async function load(db:SqlDatabase,id:string,submissionKey?:string){
 const database=db;
 const mode=await commercePackageEnforcementMode(),form=await application(database).load(id);
 const retained=mode!=='disabled'&&submissionKey?await database.prepare(`SELECT 1 FROM marketing_public_receipts receipt JOIN (${crmMarketingCandidateSources}) candidate ON candidate.id=receipt.candidate_id WHERE receipt.key=? AND candidate.company_id=? AND candidate.form_id=? AND candidate.form_version=?`).bind(submissionKey,form.company_id,form.id,form.version).first():null;
 await checkPurchasedCapability(database,mode,{companyId:form.company_id,capability:'sales.marketing',purpose:retained?'history':'new-work'});await assertProductionModuleEnabled(database,form.company_id,'marketing');return form;
}
export async function loadPublicMarketingForm(db:SqlDatabase,id:string,submissionKey?:string){try{return await withPublicMarketingFormRead(db,id,submissionKey,database=>load(database,id,submissionKey));}catch(error){return translate(error);}}
/** Submission preparation is not a public presentation response. The existing
 * command takes its own complete source leases and validates this captured
 * revision before consent/effects; retain its original stale-submit outcome. */
export async function loadPublicMarketingSubmissionForm(db:SqlDatabase,id:string,submissionKey:string){try{return await load(db,id,submissionKey);}catch(error){return translate(error);}}
