import 'server-only';

import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {hasModuleAccess} from '@erpx/catalogue/module-access';
import {ApiError} from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";

import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {marketingExpenseSources,prepareMarketingExpenseGuard} from "@erpx/server-finance/public/marketing-expenses";
import {createMarketingCampaignApplication,type MarketingCampaignMutation} from "@erpx/server-marketing/application/campaigns";
import {marketingCampaignCommand} from "@erpx/server-marketing/domain/campaigns";
import {requireMarketingPurchasedRead,marketingFinanceHistory,withMarketingPurchasedCommand} from "@erpx/server-marketing/adapters/purchases";
export type {MarketingCostSource} from "@erpx/server-marketing/domain/campaigns";
function access(a:WorkContext,write=false){requirePermission(a,write?'records.write':'records.read');if(!hasModuleAccess(a.modules,'marketing'))throw new ApiError(403,'MODULE_FORBIDDEN','Marketing access required.');}
function application(a:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;const database=()=>resource??=holdingResources(a.holdingId).then(x=>x.write);return createMarketingCampaignApplication({clock:{now:()=>Date.now()},authorize:write=>access(a,write),authorizeFinance:()=>{requirePermission(a,'finance.read');},
async campaigns(){return ((await database()).prepare('SELECT id,code,name,version FROM marketing_campaigns WHERE company_id=? ORDER BY code').bind(a.companyId).all<Row>()).then(x=>x.results);},
costSources:async()=>marketingExpenseSources(await database(),a.companyId),replay:async(key,operation,id,input)=>replayCommand(await database(),a,key,operation,id,input),commit:async(mutation,result)=>commit(await database(),a,mutation,result),
});}
async function commit<Result>(write:SqlDatabase,a:WorkContext,m:MarketingCampaignMutation,result:Result):Promise<Result>{const {input:i,key,id,operation:op,now}=m;
 const steps=i.action==='create'?[write.prepare('INSERT INTO marketing_campaigns(id,company_id,code,name,created_by,created_at) VALUES(?,?,?,?,?,?)').bind(id,a.companyId,i.code,i.name,a.userId,now)]:[
  assertion(write,key,'campaign','EXISTS(SELECT 1 FROM marketing_campaigns WHERE id=? AND company_id=?)',[i.campaignId,a.companyId]),
  prepareMarketingExpenseGuard(write,key,a.companyId,i.journalLineId),
  write.prepare('INSERT INTO marketing_campaign_costs(id,company_id,campaign_id,journal_line_id,evidence,reviewed_by,created_at) VALUES(?,?,?,?,?,?,?)').bind(id,a.companyId,i.campaignId,i.journalLineId,i.evidence,a.userId,now)
 ];
 return executeCommand(write,a,key,op,i,id,1,result,steps);
}
export async function marketingCampaigns(a:WorkContext){const db=(await holdingResources(a.holdingId)).write;await requireMarketingPurchasedRead(db,a);return application(a,db).campaigns();}
export async function marketingCostSources(a:WorkContext){const db=(await holdingResources(a.holdingId)).write;await requireMarketingPurchasedRead(db,a);return await marketingFinanceHistory(db,a)?application(a,db).costSources():[];}
export function changeMarketingCampaign(a:WorkContext,raw:unknown,key:string){const input=marketingCampaignCommand.parse(raw);return withMarketingPurchasedCommand(a,{key,operation:`marketing.campaign.${input.action}`,target:{kind:input.action==='create'?'campaign':'campaign-cost',id:input.action==='create'?`campaign-${key}`:`campaign-cost-${key}`},purpose:input.action==='create'?'new-work':'settle-existing',sources:input.action==='link-cost'?[{kind:'campaign',id:input.campaignId}]:[],finance:input.action==='link-cost'},db=>application(a,db).change(input,key));}
