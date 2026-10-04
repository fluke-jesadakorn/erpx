import 'server-only';
import {z} from 'zod';
import type {SqlDatabase,SqlStatement} from './runtime/types';
import {assertion} from './commands';
import {ApiError} from './http';

const references=z.object({contentAssetId:z.string().min(1).max(180).optional(),contentVersion:z.number().int().positive().optional(),mediaPublicationId:z.string().min(1).max(180).optional()}).superRefine((value,context)=>{
 if(Boolean(value.contentAssetId)!==Boolean(value.contentVersion))context.addIssue({code:'custom',message:'Choose an exact content revision.'});
 if(value.mediaPublicationId&&!value.contentAssetId)context.addIssue({code:'custom',message:'Choose the content owner of the public creative.'});
});
export function marketingPresentationReferences(configuration:unknown){
 let value=configuration;if(typeof value==='string'){try{value=JSON.parse(value);}catch{throw new ApiError(422,'MARKETING_PRESENTATION_INVALID','Review the saved form presentation.');}}
 const parsed=references.safeParse(value);if(!parsed.success)throw new ApiError(422,'MARKETING_PRESENTATION_INVALID',parsed.error.issues[0]?.message??'Review the saved form presentation.');return parsed.data;
}
const contentPredicate="EXISTS(SELECT 1 FROM marketing_assets WHERE id=? AND company_id=? AND kind='content' AND status='active' AND version=?)";
const mediaPredicate=`EXISTS(SELECT 1 FROM attachment_publications p
 JOIN attachment_links l ON l.id=p.source_link_id AND l.state='bound'
 JOIN attachment_sets s ON s.id=l.set_id AND s.company_id=p.company_id AND s.state='bound'
 JOIN attachments a ON a.id=l.attachment_id AND a.company_id=p.company_id AND a.source_state='available'
 WHERE p.id=? AND p.company_id=? AND p.state='approved' AND p.target_type='marketing_asset' AND p.target_id=?
 AND s.target_type=p.target_type AND s.target_id=p.target_id AND s.purpose_key='creative')`;
export function marketingPresentationAssertions(db:SqlDatabase,key:string,companyId:string,configuration:unknown):SqlStatement[]{
 const refs=marketingPresentationReferences(configuration),steps:SqlStatement[]=[];
 if(refs.contentAssetId)steps.push(db.prepare('SELECT id FROM marketing_assets WHERE id=? AND company_id=? FOR SHARE').bind(refs.contentAssetId,companyId),assertion(db,key,'form-content',contentPredicate,[refs.contentAssetId,companyId,refs.contentVersion!]));
 if(refs.mediaPublicationId)steps.push(db.prepare('SELECT id FROM attachment_publications WHERE id=? AND company_id=? FOR SHARE').bind(refs.mediaPublicationId,companyId),assertion(db,key,'form-media',mediaPredicate,[refs.mediaPublicationId,companyId,refs.contentAssetId!]));return steps;
}
export async function marketingFormReadiness(db:SqlDatabase,companyId:string,configuration:unknown){
 const refs=marketingPresentationReferences(configuration),blockers:string[]=[];
 const content=refs.contentAssetId?await db.prepare("SELECT id,version,preview_json FROM marketing_assets WHERE id=? AND company_id=? AND kind='content' AND status='active' AND version=?").bind(refs.contentAssetId,companyId,refs.contentVersion!).first<{id:string;version:number;preview_json:string}>():null;
 if(refs.contentAssetId&&!content)blockers.push('reviewed_content_unavailable');
 const media=refs.mediaPublicationId?await db.prepare(`SELECT p.id,p.alt_text,p.revision,p.derivative_media_type FROM attachment_publications p WHERE p.id=? AND ${mediaPredicate}`).bind(refs.mediaPublicationId,refs.mediaPublicationId,companyId,refs.contentAssetId!).first<{id:string;alt_text:string;revision:number;derivative_media_type:string}>():null;
 if(refs.mediaPublicationId&&!media)blockers.push('approved_creative_unavailable');
 const preview=content?JSON.parse(content.preview_json) as Record<string,unknown>:null;
 return {ready:blockers.length===0,blockers,content:content?{id:content.id,version:content.version,headline:typeof preview?.headline==='string'?preview.headline:null,summary:typeof preview?.summary==='string'?preview.summary:null}:null,media:media?{id:media.id,altText:media.alt_text,revision:media.revision,mediaType:media.derivative_media_type}:null};
}
export async function loadPublicMarketingForm(db:SqlDatabase,formId:string){
 const row=await db.prepare("SELECT id,company_id,name,version,preview_json,configuration_json FROM marketing_assets WHERE id=? AND kind='form' AND status='active'").bind(formId).first<{id:string;company_id:string;name:string;version:number;preview_json:string;configuration_json:string}>();
 if(!row||JSON.parse(row.configuration_json).consentRequired!==true)throw new ApiError(404,'FORM_NOT_FOUND','Form not found.');
 const presentation=await marketingFormReadiness(db,row.company_id,row.configuration_json);if(!presentation.ready)throw new ApiError(404,'FORM_NOT_READY','The approved form presentation is currently unavailable.');
 return {...row,presentation};
}
