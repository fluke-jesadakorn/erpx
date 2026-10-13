import {describe,expect,test} from "vitest";
import {createAgreementApplication,type AgreementPorts} from "../../../src/server/modules/field_service/application/agreement";
import {createDuplicateRequestApplication,type DuplicateRequestPorts} from "../../../src/server/modules/customer_authority/application/duplicate-request";
import {createQueuePolicyApplication,type QueuePolicyPorts} from "../../../src/server/modules/operations/application/queue-policy";
import {createAssetApplication,type AssetPorts} from "../../../src/server/modules/marketing/application/asset";
import {formRequiresConsent} from "../../../src/server/modules/marketing/domain/asset";
import {nextOccurrence,wallClock} from "../../../src/server/modules/maintenance/domain/recurrence";
import {createMarketingApplication,type MarketingRepository,type MarketingMutation} from "../../../src/server/modules/marketing/application/marketing";
import {createMarketingCampaignApplication} from "../../../src/server/modules/marketing/application/campaigns";
import {createMarketingPresentationApplication,type MarketingPresentationPorts} from "../../../src/server/modules/marketing/application/public-form";

type Command=Parameters<AgreementPorts["persist"]>[0]|Parameters<DuplicateRequestPorts["persist"]>[0]|Parameters<QueuePolicyPorts["persist"]>[0]|Parameters<AssetPorts["persist"]>[0];
function fixture(){const commands:Command[]=[],calls:string[]=[];const ports={clock:{now:()=>42},branchId:"actor-branch" as string|null,departmentId:"actor-department" as string|null,companyScope:true,async authorize(){calls.push("authorize");},validatePresentation(){calls.push("presentation");},async replay():Promise<{id:string;version:number;status:string}|null>{calls.push("replay");return null;},async persist(command:Command):Promise<{id:string;version:number;status:string}>{commands.push(command);return {id:command.id,version:1,status:"draft"};}};return {ports,calls,commands};}
const agreement={branchId:"input-branch",departmentId:"input-department",customerId:"customer",title:"Service agreement",startDate:"2026-10-01",endDate:"2026-10-31",currency:"thb",visitRateMinor:5000};

describe("remaining business owners use semantic ports",()=>{
 test("Field Service retains selected assignment scope, normalized input and original receipt identity",async()=>{
  const f=fixture();await createAgreementApplication(f.ports)(agreement,"receipt-key");
  expect(f.commands[0]).toMatchObject({id:"service-agreement-receipt-key",operation:"field.agreement.create",branch:"actor-branch",department:"actor-department",now:42,input:{currency:"THB",warrantyDiscountBps:0}});
  expect(f.calls).toEqual(["authorize","replay"]);
  await expect(createAgreementApplication(f.ports)({...agreement,endDate:"2026-09-30"},"invalid")).rejects.toMatchObject({code:"FIELD_SERVICE_DATE_RANGE"});
  expect(f.commands).toHaveLength(1);
 });
 test("Field Service validates date range before replay and uses input scope for a Company assignment",async()=>{
  const f=fixture();f.ports.branchId=null;f.ports.departmentId=null;
  await createAgreementApplication(f.ports)(agreement,"receipt-key");expect(f.commands[0]).toMatchObject({branch:"input-branch",department:"input-department"});
  f.ports.replay=async()=>({id:"original",version:1,status:"draft"});
  expect(await createAgreementApplication(f.ports)(agreement,"receipt-key")).toMatchObject({id:"original"});expect(f.commands).toHaveLength(1);
 });
 test("Customer Authority rejects non-Company scope and identical source records before receipt lookup",async()=>{
  const f=fixture(),input={leftCustomerId:"z",rightCustomerId:"a",evidence:"Matching customer evidence"};f.ports.companyScope=false;
  await expect(createDuplicateRequestApplication(f.ports)(input,"key")).rejects.toMatchObject({code:"CUSTOMER_COMPANY_REVIEW_REQUIRED"});expect(f.calls).toEqual(["authorize"]);
  f.ports.companyScope=true;await expect(createDuplicateRequestApplication(f.ports)({...input,rightCustomerId:"z"},"key")).rejects.toThrow("Compare two different customer records.");
  await createDuplicateRequestApplication(f.ports)(input,"key");expect(f.commands[0]).toMatchObject({id:"customer-duplicate-key",operation:"customer.duplicate.request",pair:["a","z"],input,now:42});
 });
 test("Operations normalizes only the persisted catalogue, retaining command fingerprint input",async()=>{
  const f=fixture(),input={area:"safety",name:"Safety response",sourceKinds:["asset","staff_task","asset"],targetMinutes:60};
  await createQueuePolicyApplication(f.ports)(input,"key");expect(f.commands[0]).toMatchObject({id:"queue-policy-key",operation:"operations.policy.draft",sourceKinds:["asset","staff_task"],input,now:42});
  await expect(createQueuePolicyApplication(f.ports)({...input,sourceKinds:["unknown"]},"other")).rejects.toThrow();expect(f.commands).toHaveLength(1);
 });
 test("Marketing validates presentation then explicit consent before replay",async()=>{
  const f=fixture(),input={kind:"form",name:"Contact form",configuration:{consentRequired:false},preview:{}};
  await expect(createAssetApplication(f.ports)(input,"key")).rejects.toMatchObject({code:"MARKETING_CONSENT_REQUIRED"});expect(f.calls).toEqual(["authorize","presentation"]);
  await createAssetApplication(f.ports)({...input,configuration:{consentRequired:true}},"key");expect(f.commands[0]).toMatchObject({id:"marketing-form-key",operation:"marketing.asset.create",now:42,result:{status:"draft",version:1}});
  expect(formRequiresConsent('{"consentRequired":true}')).toBe(true);expect(formRequiresConsent("invalid")).toBe(false);expect(formRequiresConsent({consentRequired:"true"})).toBe(false);
 });
 test("revoked authority and lost-response replay cannot persist another owner command",async()=>{
  const f=fixture();f.ports.authorize=async()=>{throw new Error("revoked");};
  await expect(createQueuePolicyApplication(f.ports)({area:"safety",name:"Safety response",sourceKinds:["asset"],targetMinutes:60},"key")).rejects.toThrow("revoked");expect(f.commands).toEqual([]);
  f.ports.authorize=async()=>{};f.ports.replay=async()=>({id:"original-duplicate",status:"pending",version:1});
  expect(await createDuplicateRequestApplication(f.ports)({leftCustomerId:"a",rightCustomerId:"b",evidence:"Compare active sources"},"key")).toMatchObject({id:"original-duplicate"});expect(f.commands).toEqual([]);
 });
});

describe("Maintenance wall-clock recurrence",()=>{
 test("retains wall-clock definition while resolving a DST gap to the next valid minute",()=>{
  expect(nextOccurrence("2026-03-07T02:30","America/New_York",1)).toEqual({local:"2026-03-08T02:30",resolvedLocal:"2026-03-08T03:00",at:Date.parse("2026-03-08T07:00:00Z")});
 });
 test("selects the first instant in a DST fold and keeps fixed-zone cadence",()=>{
  expect(nextOccurrence("2026-10-31T01:30","America/New_York",1)).toEqual({local:"2026-11-01T01:30",resolvedLocal:"2026-11-01T01:30",at:Date.parse("2026-11-01T05:30:00Z")});
  expect(wallClock(Date.parse("2026-10-05T00:00:00Z"),"Asia/Bangkok")).toBe("2026-10-05T07:00");
 });
});

function marketingFixture(overrides:Partial<MarketingRepository<string>>={}){
 const calls:string[]=[],mutations:MarketingMutation<string>[]=[];
 const repository:MarketingRepository<string>={
  async assets(){return [];},async candidates(){return [];},async branches(){return [];},async campaignSummary(){return [];},async campaigns(){return [];},async costSources(){return [];},
  async formReadiness(){return {ready:true,blockers:[],content:null,media:null};},async publications(){return [];},async sourceForm(){return null;},preparePresentation(){return "presentation";},
  async asset(){return null;},async candidate(){return null;},async detail(){return null;},async history(){return [];},async workRows(){return [];},async evidence(){return null;},
  async replay(){return null;},async createCRMLead(){throw new Error("unexpected CRM handoff");},async commit(mutation,result){mutations.push(mutation);return result;},...overrides,
 };
 repository.transaction??=async action=>action(repository);
 const ports={actorId:"reviewer",clock:{now:()=>Date.UTC(2026,9,6)},canRead:true,canLinkCost:false,publicationsReadable:false,authorize(write?:boolean){calls.push(write?"write":"read");},async repository(){return repository;}};
 return {repository,ports,calls,mutations,app:createMarketingApplication(ports)};
}
const marketingCandidate={formId:"form",formVersion:2,branchId:"branch",contactName:"Prospective customer",contactDetail:"customer@example.com",consentEvidence:"Customer accepted the published notice",attribution:{campaignId:"campaign"},brief:"Please review the requested service"};
const candidateRow={id:"candidate",version:1,status:"pending",created_by:"creator",branch_id:"branch",contact_name:"Prospective customer",contact_detail:"customer@example.com",form_id:"form",form_version:2,attribution_json:'{"campaignId":"campaign"}',brief:"Please review the requested service"};

describe("Marketing owner application ports",()=>{
 test("workspace omits foreign expense and public creative projections when the facts deny access",async()=>{
  const f=marketingFixture({async assets(){return [{id:"content",kind:"content",status:"active",configuration_json:"{}"},{id:"form",kind:"form",status:"draft",configuration_json:"{}"}];},async costSources(){throw new Error("expense projection denied");},async publications(){throw new Error("creative projection denied");}});
  expect(await f.app.workspace()).toMatchObject({canLinkCost:false,costSources:[],contentPublications:[],formReadiness:[{id:"form",ready:true}]});
  expect(f.calls).toEqual(["read"]);
 });
 test("candidate receipt replay survives a retired source form and a newer candidate version",async()=>{
  const f=marketingFixture({async replay<Result>(){return {id:"saved",status:"pending",version:1} as Result;},async sourceForm(){throw new Error("retired form must not be read");},async candidate(){return {...candidateRow,version:9};}});
  expect(await f.app.createCandidate(marketingCandidate,"saved-key")).toEqual({id:"saved",status:"pending",version:1});
  expect(await f.app.change("candidate","candidate",{action:"handoff",expectedVersion:1,reason:"Consent independently reviewed"},"saved-key")).toEqual({id:"saved",status:"pending",version:1});
  expect(f.mutations).toEqual([]);
 });
 test("asset independent review remains a prerequisite before a receipt can be returned",async()=>{
  const f=marketingFixture({async asset(){return {kind:"content",status:"draft",version:1,created_by:"reviewer"};},async replay(){throw new Error("receipt must not precede independent review");}});
  await expect(f.app.change("asset","content",{action:"activate",expectedVersion:1,reason:"Approve this content"},"key")).rejects.toMatchObject({status:409,message:"Independent review is required."});
  expect(f.mutations).toEqual([]);
 });
 test("handoff binds CRM creation and the Marketing commit to the same supplied transaction",async()=>{
  const f=marketingFixture();const effects:string[]=[];
  const transaction:MarketingRepository<string>={...f.repository,
   async candidate(_id,lock){expect(lock).toBe(true);return candidateRow;},
   async createCRMLead(raw,key){expect(key).toBe("handoff-key-crm");expect(raw).toMatchObject({branchId:"branch",sourceKind:"form",sourceId:"candidate",campaignId:"campaign",dueDate:"2026-10-13"});effects.push("crm");return {id:"crm-lead"};},
   async commit(mutation,result){expect(mutation).toMatchObject({action:"handoff",operation:"marketing.candidate.handoff",crm:{id:"crm-lead"},now:Date.UTC(2026,9,6)});effects.push("marketing");return result;},
  };
  f.repository.transaction=async action=>{effects.push("begin");const result=await action(transaction);effects.push("commit");return result;};
  expect(await f.app.change("candidate","candidate",{action:"handoff",expectedVersion:1,reason:"Consent independently reviewed"},"handoff-key")).toEqual({id:"candidate",status:"handed_off",version:2,crmLeadId:"crm-lead"});
  expect(effects).toEqual(["begin","crm","marketing","commit"]);expect(f.mutations).toEqual([]);
 });
 test("a late Marketing failure leaves no committed handoff effects in the transaction port",async()=>{
  const f=marketingFixture();const durable:string[]=[],pending:string[]=[];
  const transaction:MarketingRepository<string>={...f.repository,async candidate(){return candidateRow;},async createCRMLead(){pending.push("crm-lead");return {id:"crm-lead"};},async commit(){throw new Error("late rollback");}};
  f.repository.transaction=async action=>{try{const result=await action(transaction);durable.push(...pending);return result;}finally{pending.length=0;}};
  await expect(f.app.change("candidate","candidate",{action:"handoff",expectedVersion:1,reason:"Consent independently reviewed"},"key")).rejects.toThrow("late rollback");
  expect(durable).toEqual([]);expect(pending).toEqual([]);
 });
 test("handoff fails before creating CRM data when a transaction port is unavailable",async()=>{
  const f=marketingFixture();delete f.repository.transaction;
  await expect(f.app.change("candidate","candidate",{action:"handoff",expectedVersion:1,reason:"Consent independently reviewed"},"key")).rejects.toMatchObject({status:503,code:"MARKETING_TRANSACTION_REQUIRED"});
  expect(f.mutations).toEqual([]);
 });
 test("campaign cost linking requires Finance access before receipt replay",async()=>{
  const calls:string[]=[];const app=createMarketingCampaignApplication({clock:{now(){throw new Error("replay must not read the clock");}},authorize(){calls.push("marketing");},authorizeFinance(){calls.push("finance");throw new Error("finance revoked");},async campaigns(){return [];},async costSources(){return [];},async replay<Result>(){calls.push("receipt");return {id:"saved"} as Result;},async commit(){throw new Error("no commit");}});
  await expect(app.change({action:"link-cost",campaignId:"campaign",journalLineId:"line",evidence:"Approved expense allocation"},"key")).rejects.toThrow("finance revoked");
  expect(calls).toEqual(["marketing","finance"]);
 });
});

describe("Marketing public form projection",()=>{
 test("missing exact content and creative revisions produce blockers without exposing the form",async()=>{
  const configuration={consentRequired:true,contentAssetId:"content",contentVersion:2,mediaPublicationId:"creative"};
  const ports:MarketingPresentationPorts={async content(company,refs){expect(company).toBe("company");expect(refs).toMatchObject({contentAssetId:"content",contentVersion:2});return null;},async media(){return null;},async form(){return {id:"form",company_id:"company",name:"Contact us",version:1,preview_json:"{}",configuration_json:JSON.stringify(configuration)};}};
  const app=createMarketingPresentationApplication(ports);
  expect(await app.readiness("company",configuration)).toEqual({ready:false,blockers:["reviewed_content_unavailable","approved_creative_unavailable"],content:null,media:null});
  await expect(app.load("form")).rejects.toMatchObject({status:404,code:"FORM_NOT_READY"});
 });
 test("valid projection returns only typed preview fields and explicit consent is still required",async()=>{
  const configuration={consentRequired:true,contentAssetId:"content",contentVersion:2,mediaPublicationId:"creative"};
  const ports:MarketingPresentationPorts={async content(){return {id:"content",version:2,preview_json:'{"headline":"Reviewed headline","summary":42,"internal":"private"}'};},async media(){return {id:"creative",alt_text:"Public image",revision:3,derivative_media_type:"image/webp"};},async form(){return {id:"form",company_id:"company",name:"Contact us",version:1,preview_json:"{}",configuration_json:JSON.stringify(configuration)};}};
  const app=createMarketingPresentationApplication(ports);
  expect((await app.load("form")).presentation).toEqual({ready:true,blockers:[],content:{id:"content",version:2,headline:"Reviewed headline",summary:null},media:{id:"creative",altText:"Public image",revision:3,mediaType:"image/webp"}});
  ports.form=async()=>({id:"form",company_id:"company",name:"Contact us",version:1,preview_json:"{}",configuration_json:'{"consentRequired":false}'});
  await expect(app.load("form")).rejects.toMatchObject({code:"FORM_NOT_FOUND"});
 });
});

import {createPublicMarketingSubmissionApplication,type PublicMarketingSubmissionUnit,type PublicMarketingSubmissionPorts} from "../../../src/server/modules/marketing/application/public-submission";
import type {PublicMarketingForm} from "../../../src/server/modules/marketing/application/public-form";
const submissionForm:PublicMarketingForm={id:'form',company_id:'company',name:'Reviewed form',version:2,preview_json:'{}',configuration_json:'{}'};
const submissionBody={contactName:'Fictional visitor',contactDetail:'visitor@fixture.invalid',brief:'Fictional service inquiry',consentAccepted:true as const,source:'declared fixture'};
function submissionFixture(overrides:Partial<PublicMarketingSubmissionUnit>={},outside:Partial<PublicMarketingSubmissionPorts>={}){
 const calls:string[]=[],times=[120000,121000];let clockCalls=0;
 const candidates:Parameters<PublicMarketingSubmissionUnit['candidate']>[]=[];
 const unit:PublicMarketingSubmissionUnit={receipt:async()=>{calls.push('receipt');return null;},current:async()=>{calls.push('current');return {id:'form'};},presentation:async()=>{calls.push('presentation');},limit:async(rate,bucket)=>{calls.push(`limit:${rate}:${bucket}`);return {count:1};},branch:async()=>{calls.push('branch');return {id:'branch'};},campaign:async()=>{calls.push('campaign');return true;},candidate:async(...args)=>{calls.push('candidate');candidates.push(args);},engagement:async()=>{calls.push('engagement');},retainReceipt:async()=>{calls.push('retain');},clearAssertions:async()=>{calls.push('clear');},...overrides};
 const ports:PublicMarketingSubmissionPorts={clock:{now:()=>times[clockCalls++]},identifiers:{next:()=>{calls.push('identifier');return 'source';}},hash:value=>value,transaction:async run=>{calls.push('transaction');const result=await run(unit);calls.push('commit');return result;},...outside};
 return {app:createPublicMarketingSubmissionApplication(ports),calls,candidates,clockCalls:()=>clockCalls};
}
describe('Public Marketing submission owns its transaction and injected time',()=>{
 test('lost-response replay returns the retained candidate without a rate charge, clock or new identifier',async()=>{const f=submissionFixture({receipt:async()=>({request_hash:JSON.stringify({formId:'form',formVersion:2,body:submissionBody}),candidate_id:'retained'})},{clock:{now(){throw Error('replay clock');}},identifiers:{next(){throw Error('replay identifier');}}});expect(await f.app.submit(submissionForm,submissionBody,'key','ip')).toEqual({id:'retained',status:'pending'});expect(f.calls).toEqual(['transaction','commit']);});
 test('the same receipt cannot be reassigned to changed input',async()=>{const f=submissionFixture({receipt:async()=>({request_hash:'other',candidate_id:'retained'})});await expect(f.app.submit(submissionForm,submissionBody,'key','ip')).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});expect(f.candidates).toEqual([]);expect(f.clockCalls()).toBe(0);});
 test('an independently changed active form is rejected before rate charging',async()=>{const f=submissionFixture({current:async()=>null});await expect(f.app.submit(submissionForm,submissionBody,'key','ip')).rejects.toMatchObject({code:'FORM_CHANGED'});expect(f.calls).toEqual(['transaction','receipt']);expect(f.clockCalls()).toBe(0);});
 test('over-limit submissions cannot select a branch or allocate a candidate',async()=>{const f=submissionFixture({limit:async()=>({count:6})});await expect(f.app.submit(submissionForm,submissionBody,'key','ip')).rejects.toMatchObject({code:'FORM_RATE_LIMIT'});expect(f.calls).toEqual(['transaction','receipt','current','presentation']);expect(f.clockCalls()).toBe(1);});
 test('a removed receiving branch refuses candidate creation',async()=>{const f=submissionFixture({branch:async()=>null});await expect(f.app.submit(submissionForm,submissionBody,'key','ip')).rejects.toMatchObject({code:'FORM_BRANCH_REQUIRED'});expect(f.candidates).toEqual([]);expect(f.calls).not.toContain('identifier');expect(f.calls).not.toContain('commit');});
 test('a foreign campaign is rejected before candidate identity and creation time',async()=>{const f=submissionFixture({campaign:async()=>null});await expect(f.app.submit(submissionForm,{...submissionBody,campaignId:'foreign'},'key','ip')).rejects.toMatchObject({code:'CAMPAIGN_UNKNOWN'});expect(f.clockCalls()).toBe(1);expect(f.calls).not.toContain('identifier');});
 test('rate buckets and retained consent use separate supplied instants and commit only after receipt clearing',async()=>{const f=submissionFixture();expect(await f.app.submit(submissionForm,submissionBody,'key','ip')).toEqual({id:'marketing-candidate-source',status:'pending'});expect(f.candidates[0]).toMatchObject(['marketing-candidate-source',submissionForm,submissionBody,'branch',JSON.stringify({campaignId:'',source:'declared fixture',capture:'public_form',formId:'form',formVersion:2}),121000]);expect(f.calls).toEqual(['transaction','receipt','current','presentation','limit:form:ip:2','branch','identifier','candidate','engagement','retain','clear','commit']);expect(f.clockCalls()).toBe(2);});
});
