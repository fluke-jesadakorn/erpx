import "server-only";
import {hasModuleAccess} from "@erpx/catalogue/module-access";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {legalPackageEnforcementMode} from "@erpx/server-commercial/public/purchase-command-boundary";
import {legalReadScope,legalPurchaseControls} from "@erpx/server-legal/adapters/purchases";
const ids=['legal-records','legal-contract-value','legal-exposure','active-legal-obligations','active-legal-holds'] as const;
export async function legalMetricHistoryAccess(database:SqlDatabase,actor:WorkContext):Promise<Record<string,boolean>>{
 const mode=await legalPackageEnforcementMode();if(mode==='disabled')return Object.fromEntries(ids.map(id=>[id,true]));
 const denied=Object.fromEntries(ids.map(id=>[id,false]));if(!hasModuleAccess(actor.modules,'legal')||!hasPermission(actor,'legal.read'))return denied;
 try{const access=await legalPurchaseControls(database,actor),contracts=access?.['legal.contracts']?.history??true,matters=access?.['legal.matters']?.history??true,obligations=access?.['legal.obligations']?.history??true,any=contracts||matters||obligations;return {'legal-records':any,'legal-contract-value':contracts,'legal-exposure':matters,'active-legal-obligations':any&&obligations,'active-legal-holds':any&&obligations};}catch(error){if(error instanceof ApiError&&[403,404].includes(error.status))return denied;throw error;}
}
export async function legalMetricReadScope(database:SqlDatabase,actor:WorkContext,id:string){if(!(await legalMetricHistoryAccess(database,actor))[id])throw new ApiError(403,'METRIC_SOURCE_UNAVAILABLE','Legal history and current private-source authority are required.');return legalReadScope(database,actor);}
