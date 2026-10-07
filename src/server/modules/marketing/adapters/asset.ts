import "server-only";
import type {WorkContext} from "@/server/modules/identity_access/public/authorization";
import type {SqlDatabase} from "@/server/modules/runtime/public/runtime/types";
import {holdingResources} from "@/server/modules/runtime/public/runtime";
import {requirePermission} from "@/server/modules/identity_access/public/iam-permissions";
import {hasModuleAccess} from "@/data/module-access";
import {executeCommand,replayCommand} from "@/server/modules/workflow/public/commands";
import {ApiError} from "@/server/modules/runtime/public/http";
import {BusinessRuleError} from "../../shared";
import {createAssetApplication, type AssetPorts} from "../application/asset";
import {marketingPresentationReferences} from "../public/marketing-public-form";
export async function createMarketingAsset(a:WorkContext,raw:unknown,key:string){
let resource:Promise<SqlDatabase>|null=null;const database=()=>resource??=(holdingResources(a.holdingId).then(x=>x.write));
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
