import 'server-only';
import {z} from 'zod';
import type {WorkContext} from './authorization';
import type {Row} from './commerce';
import {holdingResources} from './runtime';
import {requirePermission} from './iam-permissions';
import {hasModuleAccess} from '@/data/module-access';
import {ApiError} from './http';
import {assertion,executeCommand,replayCommand} from './commands';

const command=z.discriminatedUnion('action',[
 z.object({action:z.literal('create'),code:z.string().trim().min(2).max(60).regex(/^[A-Za-z0-9_-]+$/),name:z.string().trim().min(2).max(160)}),
 z.object({action:z.literal('link-cost'),campaignId:z.string().min(1),journalLineId:z.string().min(1),evidence:z.string().trim().min(3).max(2000)})
]);
function access(a:WorkContext,write=false){requirePermission(a,write?'records.write':'records.read');if(!hasModuleAccess(a.modules,'marketing'))throw new ApiError(403,'MODULE_FORBIDDEN','Marketing access required.');}
export async function marketingCampaigns(a:WorkContext){access(a);const {write}=await holdingResources(a.holdingId);const rows=(await write.prepare('SELECT id,code,name,version FROM marketing_campaigns WHERE company_id=? ORDER BY code').bind(a.companyId).all<Row>()).results;return rows;}
export type MarketingCostSource = {
 source_line_id:string;journal_number:string;entry_date:string;currency:string;
 account_code:string;account_name:string;description:string;debit_minor:string;
};
export async function marketingCostSources(a:WorkContext){
 access(a);requirePermission(a,'finance.read');
 const {write}=await holdingResources(a.holdingId);
 return (await write.prepare(`SELECT l.id source_line_id,j.journal_number,j.entry_date,j.currency,
   acc.code account_code,acc.name account_name,l.description,l.debit_minor::text debit_minor
   FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
   JOIN finance_accounts acc ON acc.id=l.account_id AND acc.company_id=j.company_id
   WHERE j.company_id=? AND j.status='posted' AND acc.category='expense' AND l.debit_minor>l.credit_minor
     AND NOT EXISTS(SELECT 1 FROM marketing_campaign_costs c WHERE c.journal_line_id=l.id)
   ORDER BY j.entry_date DESC,j.journal_number DESC,l.ordinal LIMIT 200`)
   .bind(a.companyId).all<MarketingCostSource>()).results;
}
export async function changeMarketingCampaign(a:WorkContext,raw:unknown,key:string){
 access(a,true);const i=command.parse(raw);if(i.action==='link-cost')requirePermission(a,'finance.read');
 const {write}=await holdingResources(a.holdingId),id=i.action==='create'?`campaign-${key}`:`campaign-cost-${key}`,op=`marketing.campaign.${i.action}`,prior=await replayCommand(write,a,key,op,id,i);
 if(prior)return prior;const now=Date.now();
 const steps=i.action==='create'?[write.prepare('INSERT INTO marketing_campaigns(id,company_id,code,name,created_by,created_at) VALUES(?,?,?,?,?,?)').bind(id,a.companyId,i.code,i.name,a.userId,now)]:[
  assertion(write,key,'campaign','EXISTS(SELECT 1 FROM marketing_campaigns WHERE id=? AND company_id=?)',[i.campaignId,a.companyId]),
  assertion(write,key,'posted-expense',"EXISTS(SELECT 1 FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id JOIN finance_accounts acc ON acc.id=l.account_id WHERE l.id=? AND j.company_id=? AND j.status='posted' AND acc.company_id=j.company_id AND acc.category='expense' AND l.debit_minor>l.credit_minor)",[i.journalLineId,a.companyId]),
  write.prepare('INSERT INTO marketing_campaign_costs(id,company_id,campaign_id,journal_line_id,evidence,reviewed_by,created_at) VALUES(?,?,?,?,?,?,?)').bind(id,a.companyId,i.campaignId,i.journalLineId,i.evidence,a.userId,now)
 ];
 return executeCommand(write,a,key,op,i,id,1,{id,version:1},steps);
}
