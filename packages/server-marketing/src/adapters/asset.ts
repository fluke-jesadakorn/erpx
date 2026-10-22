import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {hasModuleAccess} from "@erpx/catalogue/module-access";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {ApiError} from "@erpx/server-runtime/public/http";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {createAssetApplication, type AssetPorts} from "@erpx/server-marketing/application/asset";
import {marketingPresentationReferences} from "@erpx/server-marketing/public/marketing-public-form";
import {assetSchema} from "@erpx/server-marketing/domain/asset";
import {withMarketingPurchasedCommand} from "@erpx/server-marketing/adapters/purchases";
async function create(a:WorkContext,raw:unknown,key:string,provided?:SqlDatabase){
let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;const database=()=>resource??=(holdingResources(a.holdingId).then(x=>x.write));
const ports:AssetPorts={clock:{now:()=>Date.now()},async authorize(){requirePermission(a,"records.write");if(!hasModuleAccess(a.modules,"marketing"))throw new ApiError(403,"MODULE_FORBIDDEN","Your current role cannot access Marketing.");},validatePresentation:marketingPresentationReferences,
async replay(key,op,id,input){return replayCommand<{id:string;status:string;version:number}>(await database(),a,key,op,id,input);},
async persist(command){const write=await database();const {key,operation:op,id,input:i,now,result}=command;
  return executeCommand(write, a, key, op, i, id, 1, result, [
    write
      .prepare(
        "INSERT INTO marketing_assets VALUES(?,?,?,?,?,?,'draft',?,NULL,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        i.kind,
        i.name,
        JSON.stringify(i.configuration),
        JSON.stringify(i.preview),
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO marketing_asset_events VALUES(?,?,1,'create',?,'Asset drafted for independent review','draft',?)",
      )
      .bind(`marketing-asset-event-${key}`, id, a.userId, now),
  ]);
}
};try{return await createAssetApplication(ports)(raw,key);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}
}
export async function createMarketingAsset(a:WorkContext,raw:unknown,key:string){const input=assetSchema.parse(raw);return withMarketingPurchasedCommand(a,{key,operation:'marketing.asset.create',target:{kind:'asset',id:`marketing-${input.kind}-${key}`},purpose:'new-work'},db=>create(a,input,key,db));}
