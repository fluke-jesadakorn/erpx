import type {legacyPackagePreview} from '@erpx/server-commercial/domain/legacy-packages';
import type {CompanyPackageGrant} from '@erpx/server-commercial/domain/packages';
import type {PackageGrantBatch,PackageGrantBatchPreview} from '@erpx/server-commercial/application/package-grant-batches';
export type ProviderPackageReview={comparison:PackageGrantBatchPreview;request:PackageGrantBatch;review:{token:string;expiresAt:number}};
const fail=()=>Error('The package acknowledgement did not match the captured Company and reviewed changes. Retry the captured request.');
const text=(value:unknown):value is string=>typeof value==='string'&&!!value;
const integer=(value:unknown):value is number=>typeof value==='number'&&Number.isSafeInteger(value)&&value>=0;
function grant(raw:unknown):CompanyPackageGrant{
 const row=raw as Partial<CompanyPackageGrant>;
 if(!row||!text(row.companyId)||!text(row.packageId)||!integer(row.packageVersion)||!row.packageVersion||!['active','draining','inactive'].includes(String(row.state))||!integer(row.startsAt)||row.endsAt!==null&&!integer(row.endsAt)||!integer(row.version)||!row.version||!text(row.reason)||!text(row.evidence))throw fail();
 return {companyId:row.companyId,packageId:row.packageId,packageVersion:row.packageVersion,state:row.state!,startsAt:row.startsAt,endsAt:row.endsAt!,version:row.version,reason:row.reason.trim(),evidence:row.evidence.trim()};
}
function changes(raw:unknown,empty=false):PackageGrantBatch['changes']{
 if(!Array.isArray(raw)||!empty&&!raw.length)throw fail();const result=raw.map(item=>{if(!item||!integer(item.expectedVersion))throw fail();return {expectedVersion:item.expectedVersion,grant:grant(item.grant)};}).sort((a,b)=>a.grant.packageId.localeCompare(b.grant.packageId));if(new Set(result.map(item=>item.grant.packageId)).size!==result.length)throw fail();return result;
}
function comparison(raw:unknown,empty=false):PackageGrantBatchPreview{
 const row=raw as Partial<PackageGrantBatchPreview>;if(!row||!text(row.companyId)||!text(row.expectedRevision)||!Array.isArray(row.before)||!Array.isArray(row.after))throw fail();const before=row.before.map(grant).sort((a,b)=>a.packageId.localeCompare(b.packageId)),after=row.after.map(grant).sort((a,b)=>a.packageId.localeCompare(b.packageId)),changed=changes(row.changes,empty);
 if([...before,...after,...changed.map(item=>item.grant)].some(item=>item.companyId!==row.companyId)||new Set(before.map(item=>item.packageId)).size!==before.length||new Set(after.map(item=>item.packageId)).size!==after.length)throw fail();const expected=new Map(before.map(item=>[item.packageId,item]));for(const item of changed)expected.set(item.grant.packageId,item.grant);if(JSON.stringify([...expected.values()].sort((a,b)=>a.packageId.localeCompare(b.packageId)))!==JSON.stringify(after))throw fail();return {companyId:row.companyId,expectedRevision:row.expectedRevision,before,after,changes:changed};
}
export function providerPackageReview(raw:unknown,request:PackageGrantBatch):ProviderPackageReview{
 const row=raw as Partial<ProviderPackageReview>;if(!row?.request||row.request.companyId!==request.companyId||row.request.expectedRevision!==request.expectedRevision||row.request.idempotencyKey!==request.idempotencyKey||JSON.stringify(changes(row.request.changes))!==JSON.stringify(changes(request.changes))||!row.review||!text(row.review.token)||!integer(row.review.expiresAt))throw fail();const parsed=comparison(row.comparison);if(parsed.companyId!==request.companyId||parsed.expectedRevision!==request.expectedRevision||JSON.stringify(parsed.changes)!==JSON.stringify(changes(request.changes)))throw fail();return {comparison:parsed,request:{...request,changes:parsed.changes},review:{token:row.review.token,expiresAt:row.review.expiresAt}};
}
export type ProviderPackageCapture={contextKey:string;holdingId:string;key:string;body:string;comparison:PackageGrantBatchPreview;editVersion:number;draftImage:string};
export function captureProviderPackageCommand(review:ProviderPackageReview,holdingId:string,contextKey:string,editVersion:number,draftImage:string):ProviderPackageCapture{
 const parsed=providerPackageReview(review,review.request);return {contextKey,holdingId,key:parsed.request.idempotencyKey,body:JSON.stringify({holdingId,request:parsed.request,token:parsed.review.token}),comparison:structuredClone(parsed.comparison),editVersion,draftImage};
}
export function acknowledgeProviderPackageCommand(raw:unknown,capture:ProviderPackageCapture,current:{contextKey:string;editVersion:number;draftImage:string;blocked?:boolean}){
 if(current.blocked||current.contextKey!==capture.contextKey)throw fail();const result=comparison((raw as {receipt?:unknown})?.receipt);if(JSON.stringify(result)!==JSON.stringify(capture.comparison))throw fail();return {receipt:result,clearDraft:current.editVersion===capture.editVersion&&current.draftImage===capture.draftImage};
}

export type ProviderLegacyPackageReview={comparison:ReturnType<typeof legacyPackagePreview>;request:PackageGrantBatch|null;review:ProviderPackageReview['review']|null};
const strings=(value:unknown):value is string[]=>Array.isArray(value)&&value.every(text);
/** Parse only the published legacy comparison. The source role/rollout audit remains
 * server-owned; an acknowledgement never substitutes a different captured batch. */
export function providerLegacyPackageReview(raw:unknown,input:{companyId:string;reason:string;evidence:string}):ProviderLegacyPackageReview{
 const row=raw as Partial<ProviderLegacyPackageReview>,source=row?.comparison;
 if(!source||source.companyId!==input.companyId||!text(source.authorityRevision)||typeof source.beta!=='boolean'||!strings(source.legacyModules)||!strings(source.redundantLegacyLabels)||!strings(source.resolvedLegacyLabels)||!strings(source.requiredPackages)||!Array.isArray(source.blockers)||!Array.isArray(source.roles)||![source.authorityChanges,source.rolloutChanges,source.pricingChanges].every(value=>Array.isArray(value)&&value.length===0)||source.acceptance!=='purchase-gate-coverage-only; owner workflow/shadow acceptance remains required')throw fail();
 const packages=comparison({...source,expectedRevision:row.request?.expectedRevision??'unchanged'},true);
 if(packages.changes.some(change=>change.grant.reason!==input.reason.trim()||change.grant.evidence!==input.evidence.trim()||change.expectedVersion!==0||change.grant.version!==1||change.grant.state!=='active'))throw fail();
 const blockers=source.blockers.map(value=>{if(!value||!text(value.packageId)||!text(value.code))throw fail();return {packageId:value.packageId,code:value.code};});
 const roles=source.roles.map(value=>{
  if(!value||!text(value.assignmentId)||!integer(value.assignmentVersion)||!text(value.roleId)||!integer(value.roleRevision)||!text(value.scopeKind)||value.scopeNodeId!==null&&!text(value.scopeNodeId)||![value.modulesBefore,value.modulesAfter,value.compatibilityLabels,value.purchaseCoverageModules,value.missingPurchasedCoverage].every(strings)||!Array.isArray(value.compatibilityEvidence))throw fail();
  const compatibilityEvidence=value.compatibilityEvidence.map(proof=>{if(!proof||!text(proof.label)||!strings(proof.modules)||proof.source!=='published-permission-policy/v1')throw fail();return {label:proof.label,modules:[...proof.modules],source:proof.source};});
  return {assignmentId:value.assignmentId,assignmentVersion:value.assignmentVersion,roleId:value.roleId,roleRevision:value.roleRevision,scopeKind:value.scopeKind,scopeNodeId:value.scopeNodeId,modulesBefore:[...value.modulesBefore],modulesAfter:[...value.modulesAfter],compatibilityLabels:[...value.compatibilityLabels],purchaseCoverageModules:[...value.purchaseCoverageModules],missingPurchasedCoverage:[...value.missingPurchasedCoverage],compatibilityEvidence};
 });
 const parsed={companyId:source.companyId,authorityRevision:source.authorityRevision,beta:source.beta,legacyModules:[...source.legacyModules],redundantLegacyLabels:[...source.redundantLegacyLabels],resolvedLegacyLabels:[...source.resolvedLegacyLabels],requiredPackages:[...source.requiredPackages],before:packages.before,after:packages.after,changes:packages.changes,blockers,roles,authorityChanges:[] as never[],rolloutChanges:[] as never[],pricingChanges:[] as never[],acceptance:source.acceptance};
 if(blockers.length||!packages.changes.length){if(row.request!==null||row.review!==null)throw fail();return {comparison:parsed,request:null,review:null};}
 if(!row.request||row.request.companyId!==input.companyId||!text(row.request.idempotencyKey)||!text(row.request.expectedRevision))throw fail();
 const reviewed=providerPackageReview({comparison:packages,request:row.request,review:row.review},row.request);
 return {comparison:parsed,request:reviewed.request,review:reviewed.review};
}
export function captureProviderLegacyPackageCommand(review:ProviderLegacyPackageReview,holdingId:string,contextKey:string,editVersion:number,draftImage:string){
 if(!review.request||!review.review)throw fail();return captureProviderPackageCommand({comparison:{companyId:review.comparison.companyId,expectedRevision:review.request.expectedRevision,before:review.comparison.before,after:review.comparison.after,changes:review.comparison.changes},request:review.request,review:review.review},holdingId,contextKey,editVersion,draftImage);
}
