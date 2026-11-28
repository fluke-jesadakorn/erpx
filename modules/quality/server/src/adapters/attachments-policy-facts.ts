import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {nativeQualityReadFirst,RD_ATTACHMENT_DESCRIPTOR} from "./native-quality-read";

/** Owner-native Quality attachment descriptor. The fenced owner port owns the
 * statement and the byte-bound column projection; this adapter emits no SQL
 * and fails closed on a malformed native row. Unknown target types still
 * resolve to null, exactly as the retired TypeScript descriptor read did. */
type DescriptorRow={ company_id:string; branch_id:string|null; department_id:null; version:number; };
const descriptorTargets=new Set(["quality_hold","inventory_quality_hold"]);
const invalid=():never=>{throw new ApiError(502,"QUALITY_NATIVE_DESCRIPTOR_RESPONSE_INVALID","The native Quality attachment descriptor is invalid.");};
async function descriptor(db:SqlDatabase,targetId:string,companyId:string):Promise<DescriptorRow|null>{
 const row=await nativeQualityReadFirst<Record<string,unknown>>(RD_ATTACHMENT_DESCRIPTOR,[targetId,companyId],db);
 if(row===null)return null;
 if(typeof row.company_id!=="string"||!(row.branch_id===null||typeof row.branch_id==="string")||row.department_id!==null||typeof row.version!=="number"||!Number.isSafeInteger(row.version))return invalid();
 return {company_id:row.company_id,branch_id:row.branch_id,department_id:null,version:row.version};
}
export function policyQualityFacts(db:SqlDatabase){return {
readDescriptor:(facts:{targetId:string;companyId:string;targetType:string;}):Promise<{company_id:string;branch_id:string|null;department_id:string|null;version:number}|null>=>descriptorTargets.has(facts.targetType)?descriptor(db,facts.targetId,facts.companyId):Promise.resolve(null),
readQualityHold:(facts:{targetId:string;companyId:string;}):Promise<{company_id:string;branch_id:string|null;department_id:null;version:number}|null>=>descriptor(db,facts.targetId,facts.companyId),
};}
