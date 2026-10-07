import { BusinessRuleError } from "../../shared";
import { marketingPresentationReferences, type MarketingPresentationReferences } from "../domain/presentation";
export type MarketingContent = { id:string;version:number;preview_json:string };
export type MarketingCreative = { id:string;alt_text:string;revision:number;derivative_media_type:string };
export type PublicMarketingForm = { id:string;company_id:string;name:string;version:number;preview_json:string;configuration_json:string };
export interface MarketingPresentationPorts {
 content(companyId:string,refs:MarketingPresentationReferences):Promise<MarketingContent|null>;
 media(companyId:string,refs:MarketingPresentationReferences):Promise<MarketingCreative|null>;
 form(id:string):Promise<PublicMarketingForm|null>;
}
export function createMarketingPresentationApplication(ports:MarketingPresentationPorts){
 async function readiness(companyId:string,configuration:unknown){
  const refs=marketingPresentationReferences(configuration),blockers:string[]=[];
  const content=refs.contentAssetId?await ports.content(companyId,refs):null;
  if(refs.contentAssetId&&!content)blockers.push("reviewed_content_unavailable");
  const media=refs.mediaPublicationId?await ports.media(companyId,refs):null;
  if(refs.mediaPublicationId&&!media)blockers.push("approved_creative_unavailable");
  const preview=content?JSON.parse(content.preview_json) as Record<string,unknown>:null;
  return {ready:blockers.length===0,blockers,content:content?{id:content.id,version:content.version,headline:typeof preview?.headline==="string"?preview.headline:null,summary:typeof preview?.summary==="string"?preview.summary:null}:null,media:media?{id:media.id,altText:media.alt_text,revision:media.revision,mediaType:media.derivative_media_type}:null};
 }
 return {readiness,async load(id:string){
  const row=await ports.form(id);if(!row||JSON.parse(row.configuration_json).consentRequired!==true)throw new BusinessRuleError(404,"FORM_NOT_FOUND","Form not found.");
  const presentation=await readiness(row.company_id,row.configuration_json);if(!presentation.ready)throw new BusinessRuleError(404,"FORM_NOT_READY","The approved form presentation is currently unavailable.");
  return {...row,presentation};
 }};
}
