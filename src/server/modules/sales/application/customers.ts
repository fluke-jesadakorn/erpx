import type {Row} from '../../commerce/public/commerce';import type {ListPage} from '../../work_projection/public/list-page';import type {customerTimeline} from '../../customer_360/public/customer-timeline';import type {CustomerNotesView} from '../domain/notes';import type {WorkItem} from '@/lib/work-item';import type {Clock} from '../../shared';import {BusinessRuleError} from '../../shared';
import {queryValue,salesCustomerAccess,type HubQuery} from '../domain/navigation';import type {CustomerSummary,CustomerDirectoryRow} from '../domain/customers';
type SalesHubAccess=ReturnType<typeof salesCustomerAccess>;
type Count={customer_id:string;count:number};
export interface SalesCustomerRepository {
 orderCounts(ids:string[]):Promise<{party_id:string;count:number}[]>;followupHeads(ids:string[]):Promise<{party_id:string;id:string;due_date:string;next_action:string;order_id:string}[]>;opportunityCounts(ids:string[]):Promise<Count[]>;supportCounts(ids:string[]):Promise<Count[]>;
 directory(search:string,status:string,cursor?:string):Promise<ListPage<Row&{id:string;page_key:string}>>;
 customer(id:string):Promise<Row|null>;orders(id:string):Promise<Row[]>;followups(id:string):Promise<Row[]>;opportunities(id:string):Promise<Row[]>;cases(id:string):Promise<Row[]>;supportSource(id:string):Promise<{id:string}|null>;
 timeline(id:string,cursor?:string):Promise<Awaited<ReturnType<typeof customerTimeline>>>;notes(id:string):Promise<CustomerNotesView>;reviews():Promise<{kind:string;count:number}[]>;
 crmWork():Promise<WorkItem[]>;followupWork():Promise<WorkItem[]>;orderWork():Promise<WorkItem[]>;followupIds(orderId:string):Promise<string[]>;
}
export function createSalesCustomerApplication(ports:{authorize():void;access():Promise<SalesHubAccess>;canReadSupport:boolean;canCreateSupport:boolean;canSell:boolean;timelineUnavailable(error:unknown):boolean;clock:Clock;repository():Promise<SalesCustomerRepository>}){
 async function summaries(ids:string[]):Promise<Map<string,CustomerSummary>>{
  ports.authorize();const access=await ports.access(),result=new Map(ids.map(id=>[id,{nextFollowup:null,openOrders:0,openOpportunities:access.pipeline?0:null,openSupport:access.support&&ports.canReadSupport?0:null} as CustomerSummary]));if(!ids.length)return result;
  const repository=await ports.repository(),[orders,followups]=await Promise.all([repository.orderCounts(ids),repository.followupHeads(ids)]);
  for(const row of orders)result.get(row.party_id)!.openOrders=Number(row.count);for(const row of followups)result.get(row.party_id)!.nextFollowup={id:row.id,dueDate:row.due_date,action:row.next_action,orderId:row.order_id};
  if(access.pipeline)for(const row of await repository.opportunityCounts(ids))result.get(row.customer_id)!.openOpportunities=Number(row.count);
  if(access.support&&ports.canReadSupport)for(const row of await repository.supportCounts(ids))result.get(row.customer_id)!.openSupport=Number(row.count);return result;
 }
 return {
  access:ports.access,summaries,
  async directory(query:HubQuery){ports.authorize();const repository=await ports.repository(),search=queryValue(query,'q').trim().slice(0,200),status=queryValue(query,'status');if(status&&!['active','blocked'].includes(status))throw new BusinessRuleError(422,'CUSTOMER_STATUS_INVALID','Choose an available customer status.');const page=await repository.directory(search,status,queryValue(query,'cursor')||undefined),values=await summaries(page.items.map(row=>row.id));return {...page,items:page.items.map(row=>({...row,summary:values.get(row.id)!} as CustomerDirectoryRow))};},
  async profile(id:string,query:HubQuery={}){ports.authorize();const repository=await ports.repository(),customer=await repository.customer(id);if(!customer)throw new BusinessRuleError(404,'CUSTOMER_NOT_FOUND','Customer is not available in this workspace.');const values=await summaries([id]),access=await ports.access(),orders=await repository.orders(id),followups=await repository.followups(id),opportunities=access.pipeline?await repository.opportunities(id):null;
   let timeline:Awaited<ReturnType<typeof customerTimeline>>|null=null,notes:CustomerNotesView|null=null;
   if(access.directory){try{timeline=await repository.timeline(id,queryValue(query,'cursor')||undefined);}catch(error){if(!ports.timelineUnavailable(error))throw error;}if(timeline)notes=await repository.notes(id);}
   const cases=access.support&&ports.canReadSupport?await repository.cases(id):null,supportSource=access.support&&ports.canCreateSupport?await repository.supportSource(id):null;return {customer,summary:values.get(id)!,orders,followups,opportunities,cases,timeline,notes,supportSource};
  },
  async reviews(){if(!(await ports.access()).reviews)return null;const rows=await (await ports.repository()).reviews();return {identity:Number(rows.find(row=>row.kind==='identity')?.count??0),duplicates:Number(rows.find(row=>row.kind==='duplicates')?.count??0),access:Number(rows.find(row=>row.kind==='access')?.count??0)};},
  async today(query:HubQuery){ports.authorize();const access=await ports.access(),kind=queryValue(query,'kind','all'),jobs:{label:string;load:()=>Promise<WorkItem[]>}[]=[];
   if(!['all','crm','followups','documents'].includes(kind))throw new BusinessRuleError(422,'SALES_TASK_KIND_INVALID','Choose an available task type.');if((['crm','followups'].includes(kind)&&!access.pipeline)||(kind==='documents'&&!access.documents))throw new BusinessRuleError(403,'SALES_TASK_ACCESS_DENIED','This task type is not available in your workspace.');
   const repository=await ports.repository();if(access.pipeline&&['all','crm'].includes(kind))jobs.push({label:'CRM activities',load:()=>repository.crmWork()});if(access.pipeline&&['all','followups'].includes(kind))jobs.push({label:'Customer follow-ups',load:()=>repository.followupWork()});if(access.documents&&['all','documents'].includes(kind))jobs.push({label:'Sales documents',load:()=>repository.orderWork()});
   const sources=await Promise.allSettled(jobs.map(job=>job.load())),unavailable:string[]=[],items:WorkItem[]=[];sources.forEach((source,index)=>{if(source.status==='fulfilled')items.push(...source.value);else unavailable.push(jobs[index].label);});
   const team=queryValue(query,'owner')==='team',all=queryValue(query,'status')==='all',search=queryValue(query,'q').trim().toLowerCase(),orderId=queryValue(query,'orderId'),allowedFollowups=orderId?new Set(await repository.followupIds(orderId)):null;
   const rows=items.filter(item=>(!allowedFollowups||item.type!=='customer_followup'||allowedFollowups.has(item.id))&&(all||item.queue!=='completed')&&(team||item.relationships?.some(relation=>['requested','assigned','approval'].includes(relation)))&&(!search||`${item.title} ${item.nextAction} ${item.responsibleQueue}`.toLowerCase().includes(search)));rows.sort((a,b)=>(a.dueAt??Infinity)-(b.dueAt??Infinity)||b.updatedAt-a.updatedAt||a.id.localeCompare(b.id));
   return {items:rows,unavailable,observedAt:ports.clock.now(),canCreateFollowup:access.pipeline&&ports.canSell,canReadCrm:access.pipeline,canReadDocuments:access.documents};
  }
 };
}
