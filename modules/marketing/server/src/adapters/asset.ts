import { nativeCreateAsset } from '@erpx/marketing-ui/adapters/server/native-marketing-wave2';
import "server-only";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {holdingResources} from "../../../../../platform/runtime/server/src/public/runtime";
import {requirePermission} from "@erpx/identity-access-server/public/iam-permissions";
import {hasModuleAccess} from "@erpx/platform-catalogue/module-access";
import {executeCommand,replayCommand} from "@erpx/workflow-server/public/commands";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {BusinessRuleError} from "@erpx/platform-shared-server/index";
import {createAssetApplication, type AssetPorts} from "@erpx/marketing-server/application/asset";
import {marketingPresentationReferences} from "@erpx/marketing-server/public/marketing-public-form";
import {assetSchema} from "@erpx/marketing-server/domain/asset";
import {withMarketingPurchasedCommand} from "@erpx/marketing-server/adapters/purchases";
async function create(a:WorkContext,raw:unknown,key:string,provided?:SqlDatabase){
let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;const database=()=>resource??=(holdingResources(a.holdingId).then(x=>x.write));
const ports:AssetPorts={clock:{now:()=>Date.now()},async authorize(){requirePermission(a,"records.write");if(!hasModuleAccess(a.modules,"marketing"))throw new ApiError(403,"MODULE_FORBIDDEN","Your current role cannot access Marketing.");},validatePresentation:marketingPresentationReferences,
async replay(key,op,id,input){return replayCommand<{id:string;status:string;version:number}>(await database(),a,key,op,id,input);},
async persist(command){const {key,input:i,result}=command;
  return await nativeCreateAsset(a,{kind:i.kind,name:i.name,configuration:i.configuration,preview:i.preview},key) as typeof result;
}
};try{return await createAssetApplication(ports)(raw,key);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}
}
export async function createMarketingAsset(a:WorkContext,raw:unknown,key:string){const input=assetSchema.parse(raw);return withMarketingPurchasedCommand(a,{key,operation:'marketing.asset.create',target:{kind:'asset',id:`marketing-${input.kind}-${key}`},purpose:'new-work'},db=>create(a,input,key,db));}
