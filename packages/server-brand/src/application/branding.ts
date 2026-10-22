import type { Clock } from "@erpx/server-shared/index";
import { BusinessRuleError } from "@erpx/server-shared/index";
import { brandPresentation,storefrontPresentation,validateBrandImageDimensions,validateStorefrontChange,type BrandActor,type BrandProfileRow,type DocumentBrandMode,type BrandStorefrontRow,type WorkspaceBrand,type StorefrontAppBrand,type BrandUploadIntent,type BrandProfileInput,type StorefrontBrandInput } from "@erpx/server-brand/domain/branding";
import type { ActiveBrandIdentity } from "@erpx/server-brand/domain/identity";
export type BrandOwnerType="holding_brand"|"branch_brand"|"company_storefront_brand";
export type BrandRecordKind="order"|"invoice"|"payment"|"journal"|"expense";
export type BrandUploadResult={id:string;mediaType:string;bytes:number;checksum:string};
export type BrandMutation = (
 | {action:"upload-profile"|"upload-storefront";intent:BrandUploadIntent;attachmentId:string;previousId:string|null|undefined;finalKey:string;checksum:string;bytes:number;now:number;result:BrandUploadResult}
 | {action:"profile";input:BrandProfileInput;current:BrandProfileRow|null;previousId:string|null|undefined;previousPublicationId:string|null|undefined;now:number;result:{scopeKind:"holding"|"branch";scopeId:string;documentMode:DocumentBrandMode}}
 | {action:"storefront";input:StorefrontBrandInput;current:BrandStorefrontRow;shortName:string|undefined;previousId:string|null;previousPublicationId:string|null;now:number;result:{companyId:string;shortName:string;accentColor:string;logoRemoved:boolean}}
);
export interface BrandingPorts<Bytes extends Uint8Array>{
 actor:BrandActor;clock:Clock;legacyAttachmentsReadable:boolean;
 urls:{attachment(id:string|null):string|null;publication(id:string|null):string|null};
 holdingAuthority():Promise<boolean>;organizationAuthority():Promise<boolean>;evidenceAuthority():Promise<boolean>;assertMediaWritable():void;
 company():Promise<{display_name:string}|null>;branch(id:string):Promise<{id:string;display_name:string}|null>;companyExists(id:string):Promise<boolean>;
 profile(kind:"holding"|"branch",id:string):Promise<BrandProfileRow|null>;branches():Promise<{id:string;display_name:string}[]>;profiles():Promise<BrandProfileRow[]>;
 storefront():Promise<BrandStorefrontRow|null>;storefrontLogo(companyId:string):Promise<{logo_attachment_id:string|null}|null>;published():Promise<ActiveBrandIdentity|null>;recordBranch(kind:BrandRecordKind,id:string):Promise<string|null>;
 evidence(branchId:string):Promise<{id:string;branch_name:string;document_mode:DocumentBrandMode;updated_at:number|string|bigint;primary_attachment_id:string|null;inverse_attachment_id:string|null}|null>;
 image:{metadata(buffer:ArrayBuffer):Promise<{width?:number;height?:number}>;encode(buffer:ArrayBuffer):Promise<Bytes>};
 checksum(bytes:Bytes):Promise<string>;attachmentObjectKey(id:string):Promise<string|null>;
 files:{put(key:string,bytes:Bytes,checksum:string):Promise<void>;delete(key:string):Promise<void>};
 commit<Result>(mutation:BrandMutation,result:Result):Promise<Result>;
}
export async function normalizeBrandMedia<Bytes extends Uint8Array>(image:BrandingPorts<Bytes>["image"],buffer:ArrayBuffer):Promise<Bytes>{
 let metadata;try{metadata=await image.metadata(buffer);}catch{throw new BusinessRuleError(422,"LOGO_INVALID","Upload a valid PNG or WebP image.");}
 validateBrandImageDimensions(metadata);return image.encode(buffer);
}
export function createBrandingApplication<Bytes extends Uint8Array>(ports:BrandingPorts<Bytes>){
 const a=ports.actor;
 const present=(kind:"holding"|"branch",id:string,label:string,profile:BrandProfileRow|null):WorkspaceBrand=>brandPresentation(kind,id,label,profile,ports.urls,ports.legacyAttachmentsReadable);
 async function manage(type:BrandOwnerType,id:string){
  const allowed=type==="holding_brand"?id===a.holdingId&&await ports.holdingAuthority():type==="branch_brand"?await ports.organizationAuthority()&&!!await ports.branch(id):id===a.companyId&&await ports.organizationAuthority()&&await ports.companyExists(id);
  if(!allowed)throw new BusinessRuleError(403,"BRAND_MANAGEMENT_FORBIDDEN","A Holding owner or authorized organization administrator is required.");
 }
 const normalize=(buffer:ArrayBuffer)=>normalizeBrandMedia(ports.image,buffer);
 async function storefront():Promise<StorefrontAppBrand>{
  const current=await ports.storefront();if(!current)throw new BusinessRuleError(409,"COMPANY_REQUIRED","The Company storefront is unavailable.");
  return storefrontPresentation(a,current,await ports.published(),ports.urls,ports.legacyAttachmentsReadable);
 }
 return {
  manage,normalize,storefront,recordBranch:ports.recordBranch,
  async workspace(){if(a.branchId){const branch=await ports.branch(a.branchId);if(branch)return present("branch",branch.id,branch.display_name,await ports.profile("branch",branch.id));}return present("holding",a.holdingId,a.holdingName,await ports.profile("holding",a.holdingId));},
  async document(scope:{branchId:string|null;requestedMode?:string|null}){
   const company=await ports.company();if(!company)throw new BusinessRuleError(409,"COMPANY_REQUIRED","The legal Company is unavailable.");
   const holding=present("holding",a.holdingId,a.holdingName,await ports.profile("holding",a.holdingId));
   if(!scope.branchId)return {mode:"holding" as const,companyName:company.display_name,branchName:null,holding,branch:null};
   const branchRow=await ports.branch(scope.branchId);if(!branchRow)throw new BusinessRuleError(404,"BRANCH_NOT_FOUND","The document Branch is unavailable.");
   const profile=await ports.profile("branch",branchRow.id),branch=present("branch",branchRow.id,branchRow.display_name,profile);
   const requested=["holding","branch","both"].includes(scope.requestedMode??"")?scope.requestedMode as DocumentBrandMode:null;
   return {mode:requested??profile?.document_mode??"branch",companyName:company.display_name,branchName:branchRow.display_name,holding,branch};
  },
  async list(){const branches=await ports.branches(),profiles=await ports.profiles();return {holding:present("holding",a.holdingId,a.holdingName,profiles.find(x=>x.scope_kind==="holding")??null),branches:branches.map(branch=>present("branch",branch.id,branch.display_name,profiles.find(x=>x.scope_kind==="branch"&&x.scope_id===branch.id)??null)),storefront:await storefront()};},
  async completeUpload(intent:BrandUploadIntent,buffer:ArrayBuffer){
   await manage(intent.owner_type,intent.owner_id);ports.assertMediaWritable();const normalized=await normalize(buffer),checksum=await ports.checksum(normalized);
   const isStorefront=intent.owner_type==="company_storefront_brand",kind=intent.owner_type==="holding_brand"?"holding":"branch";
   const current=isStorefront?await ports.storefrontLogo(intent.owner_id):await ports.profile(kind,intent.owner_id);
   const previousId=isStorefront?(current as {logo_attachment_id:string|null}|null)?.logo_attachment_id??null:intent.purpose==="inverse"?(current as BrandProfileRow|null)?.inverse_attachment_id:(current as BrandProfileRow|null)?.primary_attachment_id;
   const previousKey=previousId?await ports.attachmentObjectKey(previousId):null;
   const finalKey=isStorefront?`${a.holdingId}/branding/storefront/${intent.owner_id}/logo-${intent.id}.webp`:`${a.holdingId}/branding/${kind}/${intent.owner_id}/${intent.purpose}-${intent.id}.webp`;
   await ports.files.put(finalKey,normalized,checksum);const attachmentId=`attachment-${intent.id}`,now=ports.clock.now(),result={id:attachmentId,mediaType:"image/webp",bytes:normalized.byteLength,checksum};
   try{await ports.commit({action:isStorefront?"upload-storefront":"upload-profile",intent,attachmentId,previousId,finalKey,checksum,bytes:normalized.byteLength,now,result},result);}catch(error){await ports.files.delete(finalKey).catch(()=>undefined);throw error;}
   if(previousKey)await ports.files.delete(previousKey).catch(()=>undefined);return result;
  },
  async updateStorefront(input:StorefrontBrandInput){
   await manage("company_storefront_brand",a.companyId);if(await ports.published())throw new BusinessRuleError(409,"BRAND_PUBLISHED_IDENTITY_CONTROLLED","Save and independently publish a Brand identity draft to change the current storefront identity.");
   if(input.removeLogo)ports.assertMediaWritable();const shortName=validateStorefrontChange(input);
   const current=await ports.storefront();if(!current)throw new BusinessRuleError(409,"COMPANY_REQUIRED","The Company storefront is unavailable.");
   const previousId=input.removeLogo?current.logo_attachment_id??null:null,previousPublicationId=input.removeLogo?current.logo_publication_id:null,now=ports.clock.now();
   const result={companyId:a.companyId,shortName:shortName??current.short_name??Array.from(current.display_name.trim()).slice(0,40).join(""),accentColor:input.accentColor??current.accent_color??"#3659c9",logoRemoved:Boolean(input.removeLogo)};
   await ports.commit({action:"storefront",input,current,shortName,previousId,previousPublicationId,now,result},result);return {result,previousId};
  },
  async updateProfile(input:BrandProfileInput){
   await manage(input.scopeKind==="holding"?"holding_brand":"branch_brand",input.scopeId);if(input.remove)ports.assertMediaWritable();
   if(input.scopeKind==="holding"&&input.documentMode&&input.documentMode!=="holding")throw new BusinessRuleError(422,"BRAND_MODE_INVALID","Holding branding uses Holding mode.");
   const current=await ports.profile(input.scopeKind,input.scopeId),previousId=input.remove==="primary"?current?.primary_attachment_id:input.remove==="inverse"?current?.inverse_attachment_id:null,previousPublicationId=input.remove==="primary"?current?.primary_publication_id:input.remove==="inverse"?current?.inverse_publication_id:null,now=ports.clock.now();
   const result={scopeKind:input.scopeKind,scopeId:input.scopeId,documentMode:input.scopeKind==="holding"?"holding" as const:input.documentMode??current?.document_mode??"branch"};
   await ports.commit({action:"profile",input,current,previousId,previousPublicationId,now,result},result);return {result,previousId};
  },
  async evidence(branchId:string){if(!await ports.evidenceAuthority())throw new BusinessRuleError(403,"BRAND_MANAGEMENT_FORBIDDEN","A Holding owner or authorized organization administrator is required.");return ports.evidence(branchId);},
 };
}
