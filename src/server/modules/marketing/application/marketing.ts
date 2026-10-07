import type {MarketingCostSource} from "../domain/campaigns";
import type {Clock} from "../../shared";
import {BusinessRuleError} from "../../shared";
import type {Row} from "../../commerce/public/commerce";
import {formRequiresConsent} from "../domain/asset";
import {marketingCandidateSchema,marketingActionSchema,marketingWorkItems,marketingCandidateLeadInput,type MarketingCandidateInput,type MarketingAction,type MarketingExecutiveEvidenceMetric} from "../domain/marketing";
type Common={key:string;id:string;operation:string;now:number};
export type MarketingPublication=Awaited<ReturnType<typeof import("../../files/public/attachments/service").listAttachmentPublications>>["items"][number];
export type MarketingMutation<Prepared>=Common&(
 | {action:"candidate";input:MarketingCandidateInput;presentation:Prepared|null}
 | {action:"asset";input:MarketingAction;expected:string;status:string;presentation:Prepared|null}
 | {action:"engagement";input:MarketingAction;record:Row}
 | {action:"handoff";input:MarketingAction;record:Row;crm:Record<string,unknown>}
);
export interface MarketingRepository<Prepared>{
 transaction?<Result>(action:(repository:MarketingRepository<Prepared>)=>Promise<Result>):Promise<Result>;
 assets():Promise<Row[]>;candidates():Promise<Row[]>;branches():Promise<Row[]>;campaignSummary():Promise<Row[]>;
 campaigns():Promise<Row[]>;costSources():Promise<MarketingCostSource[]>;
 formReadiness(configuration:unknown):Promise<Awaited<ReturnType<ReturnType<typeof import("./public-form").createMarketingPresentationApplication>["readiness"]>>>;
 publications(id:string):Promise<MarketingPublication[]>;
 sourceForm(id:string,version:number):Promise<{configuration_json:string}|null>;preparePresentation(key:string,configuration:unknown):Prepared;
 asset(id:string):Promise<Row|null>;candidate(id:string,lock?:boolean):Promise<Row|null>;detail(id:string):Promise<Row|null>;history(id:string):Promise<Row[]>;workRows():Promise<Row[]>;
 evidence(metric:MarketingExecutiveEvidenceMetric,id:string,range?:{from:number;toExclusive:number}):Promise<Row|null>;
 replay<Result=Record<string,unknown>>(key:string,operation:string,id:string,input:unknown):Promise<Result|null>;
 createCRMLead(raw:unknown,key:string):Promise<Record<string,unknown>>;
 commit<Result>(mutation:MarketingMutation<Prepared>,result:Result):Promise<Result>;
}
export function createMarketingApplication<Prepared>(ports:{actorId:string;clock:Clock;canRead:boolean;canLinkCost:boolean;publicationsReadable:boolean;authorize(write?:boolean):void;repository():Promise<MarketingRepository<Prepared>>}){
 return {
  async workspace(){
   ports.authorize();const repository=await ports.repository(),[assets,candidates,branches,campaignSummary]=await Promise.all([repository.assets(),repository.candidates(),repository.branches(),repository.campaignSummary()]);
   return {assets,candidates,branches,campaignSummary,campaigns:await repository.campaigns(),canLinkCost:ports.canLinkCost,costSources:ports.canLinkCost?await repository.costSources():[],
    formReadiness:await Promise.all(assets.filter(asset=>asset.kind==="form").map(async asset=>({id:String(asset.id),...await repository.formReadiness(asset.configuration_json)}))),
    contentPublications:ports.publicationsReadable?await Promise.all(assets.filter(asset=>asset.kind==="content"&&asset.status==="active").map(async asset=>({assetId:String(asset.id),items:(await repository.publications(String(asset.id))).filter(publication=>publication.state==="approved")}))):[]};
  },
  async createCandidate(raw:unknown,key:string){
   ports.authorize(true);const input=marketingCandidateSchema.parse(raw),repository=await ports.repository(),id=`marketing-candidate-${key}`,operation="marketing.candidate.create",prior=await repository.replay<{id:string;status:string;version:number}>(key,operation,id,input);if(prior)return prior;
   const source=await repository.sourceForm(input.formId,input.formVersion);
   if(source&&!(await repository.formReadiness(source.configuration_json)).ready)throw new BusinessRuleError(409,"MARKETING_FORM_NOT_READY","The approved form presentation is currently unavailable.");
   const now=ports.clock.now(),presentation=source?repository.preparePresentation(key,source.configuration_json):null;
   return repository.commit({action:"candidate",input,key,id,operation,now,presentation},{id,status:"pending",version:1});
  },
  async change(kind:"asset"|"candidate",id:string,raw:unknown,key:string,current?:MarketingRepository<Prepared>):Promise<Record<string,unknown>>{
   ports.authorize(true);const input=marketingActionSchema.parse(raw),repository=current??await ports.repository();
   if(!current&&kind==="candidate"&&input.action==="handoff"){
    if(!repository.transaction)throw new BusinessRuleError(503,"MARKETING_TRANSACTION_REQUIRED","The transaction runtime is unavailable. Retry when the service is ready.");
    return repository.transaction(tx=>this.change(kind,id,raw,key,tx));
   }
   if(kind==="asset"){
    const row=await repository.asset(id);if(!row)throw new BusinessRuleError(404,"MARKETING_ASSET_NOT_FOUND","Asset not found.");
    if(input.action==="activate"&&row.kind==="form"&&!formRequiresConsent(row.configuration_json))throw new BusinessRuleError(422,"MARKETING_CONSENT_REQUIRED","Lead capture forms must require consent before activation.");
    const expected=input.action==="retire"?"active":"draft",status=input.action==="activate"?"active":input.action==="reject"?"rejected":"retired";
    if(!["activate","reject","retire"].includes(input.action)||row.status!==expected||Number(row.version)!==input.expectedVersion)throw new BusinessRuleError(409,"CONFLICT","Asset changed or transition is invalid.");
    if(row.created_by===ports.actorId&&input.action!=="retire")throw new BusinessRuleError(409,"CONFLICT","Independent review is required.");
    const operation=`marketing.asset.${input.action}`,prior=await repository.replay(key,operation,id,input);if(prior)return prior;
    const now=ports.clock.now(),needsPresentation=input.action==="activate"&&row.kind==="form",presentation=needsPresentation?repository.preparePresentation(key,row.configuration_json):null;
    if(needsPresentation&&!(await repository.formReadiness(row.configuration_json)).ready)throw new BusinessRuleError(409,"MARKETING_FORM_NOT_READY","Independently approve the exact content and public creative before activating this form.");
    return repository.commit({action:"asset",input,id,key,operation,now,expected,status,presentation},{id,status,version:input.expectedVersion+1});
   }
   const row=await repository.candidate(id,input.action==="handoff");if(!row)throw new BusinessRuleError(404,"MARKETING_CANDIDATE_NOT_FOUND","Lead candidate not found.");
   const operation=input.action==="record-engagement"?"marketing.candidate.engagement":"marketing.candidate.handoff",prior=await repository.replay(key,operation,id,input);if(prior)return prior;
   if(Number(row.version)!==input.expectedVersion)throw new BusinessRuleError(409,"CONFLICT","Candidate changed. Reload and retry.");
   if(input.action==="record-engagement"){
    if(!input.eventType||!input.channel)throw new BusinessRuleError(422,"ENGAGEMENT_DETAIL_REQUIRED","Event type and channel are required.");
    return repository.commit({action:"engagement",input,id,key,operation,now:ports.clock.now(),record:row},{id,status:row.status,version:input.expectedVersion+1});
   }
   if(input.action!=="handoff"||row.status!=="pending")throw new BusinessRuleError(409,"INVALID_MARKETING_TRANSITION","Only a pending candidate can enter CRM.");
   if(row.created_by===ports.actorId)throw new BusinessRuleError(409,"CONFLICT","Independent review is required before CRM handoff.");
   const dueDate=input.dueDate??new Date(ports.clock.now()+7*86400000).toISOString().slice(0,10),crm=await repository.createCRMLead(marketingCandidateLeadInput(row,dueDate),`${key}-crm`);
   return repository.commit({action:"handoff",input,id,key,operation,now:ports.clock.now(),record:row,crm},{id,status:"handed_off",version:input.expectedVersion+1,crmLeadId:crm.id});
  },
  async detail(id:string){ports.authorize();const repository=await ports.repository(),candidate=await repository.detail(id);if(!candidate)throw new BusinessRuleError(404,"MARKETING_CANDIDATE_NOT_FOUND","Lead candidate not found.");return {candidate,history:await repository.history(id)};},
  async evidence(metric:MarketingExecutiveEvidenceMetric,id:string,range?:{from:number;toExclusive:number}){ports.authorize();if(metric!=="marketing-lead-candidates"&&!range)return null;return (await ports.repository()).evidence(metric,id,range);},
  async work(){if(!ports.canRead)return [];return marketingWorkItems(await (await ports.repository()).workRows(),ports.actorId);},
 };
}
