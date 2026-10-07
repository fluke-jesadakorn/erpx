import { BusinessRuleError } from "../../shared";
import type { ActiveBrandIdentity } from "./identity";
export type BrandActor={holdingId:string;holdingName:string;holdingSlug:string;companyId:string;branchId:string|null};
export type BrandStorefrontRow={slug:string;display_name:string;short_name:string|null;accent_color:string|null;logo_attachment_id:string|null;logo_publication_id:string|null};
export type BrandUploadIntent={id:string;owner_type:"holding_brand"|"branch_brand"|"company_storefront_brand";owner_id:string;purpose:BrandVariant|"logo";revision:number;original_name:string};
export type BrandProfileInput={scopeKind:"holding"|"branch";scopeId:string;documentMode?:DocumentBrandMode;remove?:BrandVariant;commandId:string};
export type StorefrontBrandInput={shortName?:string;accentColor?:string;removeLogo?:boolean;commandId:string};
type MediaUrls={attachment(id:string|null):string|null;publication(id:string|null):string|null};
export const documentBrandModes = ["holding", "branch", "both"] as const;
export type DocumentBrandMode = (typeof documentBrandModes)[number];
export type BrandVariant = "primary" | "inverse";

export type BrandProfileRow = {
  scope_kind: "holding" | "branch";
  scope_id: string;
  company_id: string | null;
  primary_attachment_id: string | null;
  inverse_attachment_id: string | null;
  primary_publication_id: string | null;
  inverse_publication_id: string | null;
  document_mode: DocumentBrandMode;
  updated_at: number;
};

export type WorkspaceBrand = {
  scopeKind: "holding" | "branch";
  scopeId: string;
  label: string;
  initials: string;
  primaryUrl: string | null;
  inverseUrl: string | null;
  documentMode: DocumentBrandMode;
};

export type StorefrontAppBrand = {
  companyId: string;
  holdingSlug: string;
  companySlug: string;
  companyName: string;
  shortName: string;
  accentColor: string;
  logoUrl: string | null;
  identityRevision?: number;
};

export type DocumentBrand = {
  mode: DocumentBrandMode;
  companyName: string;
  branchName: string | null;
  holding: WorkspaceBrand;
  branch: WorkspaceBrand | null;
};

function initials(label: string) {
  const words = label.trim().split(/\s+/u).filter(Boolean);
  const selected = words.length > 1 ? [words[0], words.at(-1)!] : words;
  return selected.map((word) => Array.from(word)[0] ?? "").join("").toLocaleUpperCase().slice(0, 3) || "—";
}

export function brandPresentation(kind:"holding"|"branch",id:string,label:string,profile:BrandProfileRow|null,urls:MediaUrls,legacyReadable:boolean):WorkspaceBrand{
 return {scopeKind:kind,scopeId:id,label,initials:initials(label),primaryUrl:urls.publication(profile?.primary_publication_id??null)??(legacyReadable?null:urls.attachment(profile?.primary_attachment_id??null)),inverseUrl:urls.publication(profile?.inverse_publication_id??null)??(legacyReadable?null:urls.attachment(profile?.inverse_attachment_id??null)),documentMode:kind==="holding"?"holding":profile?.document_mode??"branch"};
}
export function storefrontPresentation(actor:BrandActor,row:BrandStorefrontRow,published:ActiveBrandIdentity|null,urls:MediaUrls,legacyReadable:boolean):StorefrontAppBrand{
 const logoId=published?.identity.visual.logoPublicationId??row.logo_publication_id;
 return {companyId:actor.companyId,holdingSlug:actor.holdingSlug,companySlug:row.slug,companyName:row.display_name,shortName:published?published.identity.identity.shortName:row.short_name?.trim()||Array.from(row.display_name.trim()).slice(0,40).join(""),accentColor:published?.identity.visual.accentColor??row.accent_color??"#3659c9",logoUrl:urls.publication(published?published.identity.visual.logoPublicationId:logoId)??(published||legacyReadable?null:urls.attachment(row.logo_attachment_id)),...(published?{identityRevision:published.revision}:{})};
}
export function validateBrandImageDimensions(metadata:{width?:number;height?:number}){
 if(!metadata.width||!metadata.height||metadata.width<32||metadata.height<32)throw new BusinessRuleError(422,"LOGO_DIMENSIONS","Logo dimensions must be at least 32 by 32 pixels.");
 if(metadata.width*metadata.height>16_777_216)throw new BusinessRuleError(422,"LOGO_PIXELS","Logo dimensions must not exceed 16 megapixels.");
}
export function validateStorefrontChange(input:StorefrontBrandInput){
 const shortName=input.shortName?.trim();
 if(shortName!==undefined&&(Array.from(shortName).length<1||Array.from(shortName).length>40))throw new BusinessRuleError(422,"STOREFRONT_NAME_INVALID","Use a storefront name between 1 and 40 characters.");
 if(input.accentColor!==undefined&&!/^#[0-9a-fA-F]{6}$/.test(input.accentColor))throw new BusinessRuleError(422,"STOREFRONT_COLOR_INVALID","Use a six-digit hex color such as #3659c9.");
 if(shortName===undefined&&input.accentColor===undefined&&!input.removeLogo)throw new BusinessRuleError(422,"STOREFRONT_CHANGE_REQUIRED","Choose a name, color, or logo change.");return shortName;
}
