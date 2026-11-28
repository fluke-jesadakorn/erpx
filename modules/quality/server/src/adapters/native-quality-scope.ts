import "server-only";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {nativeQualityReadFirst} from "./native-quality-read";

/** SQL-free transport for the two retained Quality attachment owner-scope
 * reads. The owner registry plans the exact statement; this module only sends
 * the closed kind and positional values, then validates the returned row. No
 * TypeScript fallback, no SQL text, no caller-supplied predicate. */
export type QualityScopeRow={company_id:string;branch_id:string|null;department_id:string|null;version:number;};
const invalid=():never=>{throw new ApiError(502,"QUALITY_NATIVE_SCOPE_RESPONSE_INVALID","The native Quality attachment scope is invalid.");};
export async function readNativeQualityScope(kind:string,values:readonly unknown[],db:SqlDatabase):Promise<QualityScopeRow|null>{
 const row=await nativeQualityReadFirst<Record<string,unknown>>(kind,values,db);
 if(row===null)return null;
 if(typeof row.company_id!=="string"||!(row.branch_id===null||typeof row.branch_id==="string")||!(row.department_id===null||typeof row.department_id==="string")||typeof row.version!=="number"||!Number.isSafeInteger(row.version))return invalid();
 return {company_id:row.company_id,branch_id:row.branch_id,department_id:row.department_id,version:row.version};
}
