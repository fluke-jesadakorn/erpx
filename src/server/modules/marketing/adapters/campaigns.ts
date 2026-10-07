import 'server-only';

import type {WorkContext} from "@/server/modules/identity_access/public/authorization";
import type {Row} from "@/server/modules/commerce/public/commerce";
import {holdingResources} from "@/server/modules/runtime/public/runtime";
import {requirePermission} from "@/server/modules/identity_access/public/iam-permissions";
import {hasModuleAccess} from '@/data/module-access';
import {ApiError} from "@/server/modules/runtime/public/http";
import {assertion,executeCommand,replayCommand} from "@/server/modules/workflow/public/commands";

import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {marketingExpenseSources,prepareMarketingExpenseGuard} from "../../finance/public/marketing-expenses";
import {createMarketingCampaignApplication,type MarketingCampaignMutation} from "../application/campaigns";
export type {MarketingCostSource} from "../domain/campaigns";
function access(a:WorkContext,write=false){requirePermission(a,write?'records.write':'records.read');if(!hasModuleAccess(a.modules,'marketing'))throw new ApiError(403,'MODULE_FORBIDDEN','Marketing access required.');}
function application(a:WorkContext){let resource:Promise<SqlDatabase>|null=null;const database=()=>resource??=holdingResources(a.holdingId).then(x=>x.write);return createMarketingCampaignApplication({clock:{now:()=>Date.now()},authorize:write=>access(a,write),authorizeFinance:()=>{requirePermission(a,'finance.read');},
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
export function marketingCampaigns(a:WorkContext){return application(a).campaigns();}
export function marketingCostSources(a:WorkContext){return application(a).costSources();}
export function changeMarketingCampaign(a:WorkContext,raw:unknown,key:string){return application(a).change(raw,key);}
