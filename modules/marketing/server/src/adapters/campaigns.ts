import { nativeCreateCampaign } from '@erpx/marketing-ui/adapters/server/native-marketing-wave2';
import { nativeCampaignCostLink, nativeMarketingStatement } from '@erpx/marketing-ui/adapters/server/native-marketing-command';
import 'server-only';

import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {Row} from '@erpx/commerce-server/public/commerce';
import {holdingResources} from "../../../../../platform/runtime/server/src/public/runtime";
import {requirePermission} from "@erpx/identity-access-server/public/iam-permissions";
import {hasModuleAccess} from '@erpx/platform-catalogue/module-access';
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/workflow-server/public/commands";

import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {marketingExpenseSources,prepareMarketingExpenseGuard} from "@erpx/finance-server/public/marketing-expenses";
import {createMarketingCampaignApplication,type MarketingCampaignMutation} from "@erpx/marketing-server/application/campaigns";
import {marketingCampaignCommand} from "@erpx/marketing-server/domain/campaigns";
import {MARKETING_RD_CAMPAIGN_EXISTS,nativeMarketingPlan} from './native-marketing-read';
import {requireMarketingPurchasedRead,marketingFinanceHistory,withMarketingPurchasedCommand} from "@erpx/marketing-server/adapters/purchases";
export type {MarketingCostSource} from "@erpx/marketing-server/domain/campaigns";
function access(a:WorkContext,write=false){requirePermission(a,write?'records.write':'records.read');if(!hasModuleAccess(a.modules,'marketing'))throw new ApiError(403,'MODULE_FORBIDDEN','Marketing access required.');}
function application(a:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;const database=()=>resource??=holdingResources(a.holdingId).then(x=>x.write);return createMarketingCampaignApplication({clock:{now:()=>Date.now()},authorize:write=>access(a,write),authorizeFinance:()=>{requirePermission(a,'finance.read');},
async campaigns(){return (await nativeMarketingStatement('marketing_campaigns_read',{actor:a}).bind(a.companyId).all<Row>()).results;},
costSources:async()=>marketingExpenseSources(await database(),a.companyId),replay:async(key,operation,id,input)=>replayCommand(await database(),a,key,operation,id,input),commit:async(mutation,result)=>commit(await database(),a,mutation,result),
});}
async function commit<Result>(write:SqlDatabase,a:WorkContext,m:MarketingCampaignMutation,result:Result):Promise<Result>{
  const {input:i,key,id,operation:op,now}=m;
  if(i.action==='create'){await nativeCreateCampaign(a,i,key);return result;}
  const exists=await nativeMarketingPlan(MARKETING_RD_CAMPAIGN_EXISTS);
  return executeCommand(write,a,key,op,i,id,1,result,[
    assertion(write,key,'campaign',`EXISTS(${exists.sql})`,[i.campaignId,a.companyId]),
    prepareMarketingExpenseGuard(write,key,a.companyId,i.journalLineId),
    nativeCampaignCostLink(a,{id,companyId:a.companyId,campaignId:i.campaignId,journalLineId:i.journalLineId,evidence:i.evidence,reviewedBy:a.userId,now},key),
  ]);
}
export async function marketingCampaigns(a:WorkContext){const db=(await holdingResources(a.holdingId)).write;await requireMarketingPurchasedRead(db,a);return application(a,db).campaigns();}
export async function marketingCostSources(a:WorkContext){const db=(await holdingResources(a.holdingId)).write;await requireMarketingPurchasedRead(db,a);return await marketingFinanceHistory(db,a)?application(a,db).costSources():[];}
export function changeMarketingCampaign(a:WorkContext,raw:unknown,key:string){const input=marketingCampaignCommand.parse(raw);return withMarketingPurchasedCommand(a,{key,operation:`marketing.campaign.${input.action}`,target:{kind:input.action==='create'?'campaign':'campaign-cost',id:input.action==='create'?`campaign-${key}`:`campaign-cost-${key}`},purpose:input.action==='create'?'new-work':'settle-existing',sources:input.action==='link-cost'?[{kind:'campaign',id:input.campaignId}]:[],finance:input.action==='link-cost'},db=>application(a,db).change(input,key));}
