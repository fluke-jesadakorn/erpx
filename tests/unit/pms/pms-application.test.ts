import { ownedReleaseReadDiagnostics } from "../../../src/server/modules/runtime/adapters/runtime/read-request";
import {expect,it,vi} from "vitest";
import {pmsPackageCommandAllowed} from "../../../src/lib/pms/package-access";
import {ApiError,errorResponse} from "../../../src/server/modules/runtime/adapters/http";

import {createPmsReportsApplication,type PmsReportsRepository} from "../../../src/server/modules/pms/application/reports";
import {createGuestBookingDocument,type GuestDocumentUnit} from "../../../src/server/modules/pms/application/guest-document";
import {createStaffBookingChange} from "../../../src/server/modules/pms/application/staff-booking-changes";
import {createPmsRollout,type PmsRolloutRepository} from "../../../src/server/modules/pms/application/hospitality-rollout";
import {createPmsSocialProjector,type PmsSocialUnit} from "../../../src/server/modules/pms/application/social-events";
import {createPmsImport,type PmsImportRepository} from "../../../src/server/modules/pms/application/import";
import {createPmsBookingRefund,type PmsBookingRefundPorts,type PmsBookingRefundRepository} from "../../../src/server/modules/pms/application/pms-booking-refunds";
import {productionAccepted} from "../../../src/server/modules/pms/application/hospitality-evidence";
import {pmsSourceAllowed,type PmsSource} from "../../../src/server/modules/pms/domain/source";
import {newProperty,executePms} from "../../../src/lib/pms/pms-engine";
import {createPmsApplication,type PmsUnit} from "../../../src/server/modules/pms/application/pms";
import {createPmsBooking,type PmsBookingUnit,type PmsBookingRepository} from "../../../src/server/modules/pms/application/pms-booking";
import type {WorkContext} from "../../../src/server/modules/identity_access/public/authorization";
import type {EventMessage} from "../../../src/server/modules/workflow/public/job-projector";
function repository<T extends object>(operations:Partial<T>):T{return new Proxy(operations as T,{get(target,name){if(name==="then")return undefined;if(name in target)return Reflect.get(target,name);return ()=>{throw new Error(`unexpected operation: ${String(name)}`);};}});}
const error=(_status:number,code:string)=>new Error(code),clock={now:()=>100};
const actor:WorkContext={userId:"reviewer",name:"Reviewer",email:"reviewer@example.test",assignmentId:"assignment",role:"holding_owner",companyId:"company",branchId:null,departmentId:null,holdingId:"H001",holdingSlug:"test",holdingName:"Test",scope:{kind:"holding",holdingId:"H001",holdingSlug:"test",holdingName:"Test",companyId:"company"},assignmentScope:"holding",inherited:false,modules:["pms"],assignments:[],workspaces:[]};
const snapshot=()=>newProperty("hotel","company","branch","Hotel","2026-10-05");
const reports=createPmsReportsApplication({clock:{isoNow:()=>"2026-10-09T00:00:00.000Z"},require:()=>{},error});
const reportInput={propertyId:"hotel",from:"2026-10-01",to:"2026-10-02"};
it("the actual PMS endpoint rejects a Finance reservation command before acquiring the Holding database and retains the unchanged transfer command",async()=>{
 const command=vi.fn(async()=>({id:"transfer",version:2})),resources=vi.fn(async()=>({write:{}}));
 const modules=["@/server/modules/pms/public/pms","@/server/modules/identity_access/public/authorization","@/server/modules/runtime/public/runtime","@/server/modules/runtime/public/runtime/types","@/server/modules/manage_erpx/public/public-origin","@/server/modules/runtime/public/runtime/read-request","@/server/modules/runtime/public/runtime/read-metrics"];
 vi.stubEnv("ERPX_PRODUCT_ROUTING","enabled");vi.stubEnv("ERPX_PRODUCT_SSO","enabled");vi.stubEnv("ERPX_PRODUCT_ENVIRONMENT","development");vi.stubEnv("ERPX_ENVIRONMENT","local");
 vi.doMock(modules[0],()=>({pmsCommand:command,pmsWorkspace:vi.fn()}));
 vi.doMock(modules[1],()=>({requireWorkContext:async()=>actor}));
 vi.doMock(modules[2],()=>({holdingResources:resources}));
 vi.doMock(modules[3],()=>({isTransactionalSqlDatabase:()=>true}));
 vi.doMock(modules[4],()=>({requestPublicOrigin:()=>"https://finance-dev-erpx.jesadakorn.com"}));
 vi.doMock(modules[5],()=>({measureReadRequest:async(_name:string,read:()=>Promise<unknown>)=>read()}));
 vi.doMock(modules[6],()=>({measureReadPhase:async(_name:string,read:()=>Promise<unknown>)=>read()}));
 try{
  const {POST}=await import("@/app/api/v1/holdings/[holdingSlug]/pms/route");
  const request=(body:unknown)=>new Request("https://finance-dev-erpx.jesadakorn.com/api/v1/holdings/test/pms",{method:"POST",headers:{host:"finance-dev-erpx.jesadakorn.com",origin:"https://finance-dev-erpx.jesadakorn.com","Content-Type":"application/json","Idempotency-Key":"same-native-key"},body:JSON.stringify(body)});
  const denied=await POST(request({operation:"reservation.check_in",propertyId:"hotel",input:{id:"reservation"}}),{params:Promise.resolve({holdingSlug:"test"})});
  expect(denied.status).toBe(404);expect(await denied.json()).toMatchObject({error:{code:"NOT_FOUND"}});expect(resources).not.toHaveBeenCalled();expect(command).not.toHaveBeenCalled();
  const body={operation:"payment.verify_transfer",propertyId:"hotel",expectedVersion:3,input:{id:"transfer",expectedTransferVersion:1,bankTransactionId:"bank",bankVersion:2}};
  const accepted=await POST(request(body),{params:Promise.resolve({holdingSlug:"test"})});
  expect(accepted.status).toBe(200);expect(resources).toHaveBeenCalledOnce();expect(command).toHaveBeenCalledWith({},actor,body,"same-native-key");
 }finally{for(const mockedModule of modules)vi.doUnmock(mockedModule);vi.unstubAllEnvs();}
});
it("a PMS report rejects a range over ninety-three days before any owner facts",async()=>{await expect(reports.pmsRangeReport(repository<PmsReportsRepository>({}),actor,{...reportInput,to:"2027-10-01"})).rejects.toThrow();});
it("a PMS report conceals another branch before reading its money and metrics",async()=>{const r=repository<PmsReportsRepository>({property:async()=>({branch_id:"other",config_json:"{}"})});await expect(reports.pmsRangeReport(r,{...actor,branchId:"branch"},reportInput)).rejects.toThrow("PMS_NOT_FOUND");});
it("PMS report permission withdrawal prevents all repository reads",async()=>{const app=createPmsReportsApplication({clock:{isoNow:()=>"2026-10-09T00:00:00.000Z"},require:()=>{throw error(403,"REVOKED");},error});await expect(app.pmsRangeReport(repository<PmsReportsRepository>({}),actor,reportInput)).rejects.toThrow("REVOKED");});
it("PMS report aggregates integer revenue, occupancy and liability signs from provided facts",async()=>{const s=snapshot();s.property.config.accounts.receivable="ar";s.property.config.accounts.advance="advance";s.property.config.accounts.deposit="deposit";const r=repository<PmsReportsRepository>({property:async()=>({branch_id:"branch",config_json:JSON.stringify(s.property.config)}),pace:async()=>({results:[]}),channels:async()=>({results:[]}),days:async()=>({results:[{date:"2026-10-01",capacity:10,sold:4,room_revenue:10000,net_revenue:9000,tax:1000,refunds:200,cancellations:1,no_shows:1}]}),balances:async()=>({results:[{account_id:"ar",balance:4000},{account_id:"advance",balance:-3000},{account_id:"deposit",balance:-500}]})});expect(await reports.pmsRangeReport(r,actor,reportInput)).toMatchObject({occupancy:0.4,adr:2500,revpar:1000,balances:{receivable:4000,advance:3000,deposit:500}});});
it("PMS report pagination keeps exactly fifty rows and signals the next page",async()=>{const r=repository<PmsReportsRepository>({property:async()=>({branch_id:"branch",config_json:"{}"}),entries:async facts=>{expect(facts.offset).toBe(50);return {results:Array.from({length:51},(_,i)=>({id:String(i),data_json:JSON.stringify({total:i})}))};}});const result=await reports.pmsReportEntries(r,actor,"hotel","2026-10-05",2);expect(result.rows).toHaveLength(50);expect(result.hasNext).toBe(true);});
it("guest document lookup is bound to the authorized hotel and booking",async()=>{const unit=repository<GuestDocumentUnit>({authorized:async(token,now)=>{expect([token,now]).toEqual(["management",100]);return {s:snapshot(),id:"booking"};},document:async(id,property,reservation)=>{expect([id,property,reservation]).toEqual(["doc","hotel","booking"]);return null;}});const app=createGuestBookingDocument({repository:{transaction:async work=>work(unit)},clock,error});await expect(app("management",{id:"doc"})).rejects.toThrow("PMS_DOCUMENT_NOT_FOUND");});
it("guest document preserves its issued amounts and excludes Finance and personal identifiers",async()=>{const unit=repository<GuestDocumentUnit>({authorized:async()=>({s:snapshot(),id:"booking"}),document:async()=>({kind:"invoice",created_at:1,snapshot_json:JSON.stringify({property:{name:"Issued Hotel",currency:"THB",chartId:"private"},guest:{name:"Guest",identityNumber:"private"},guestAccount:{name:"Room",due:77,partyId:"private"},amount:123,businessDate:"2026-10-01",issuedBy:"private",entries:[{data:{date:"2026-10-01",description:"Room",net:100,service:10,tax:13,total:123,actorId:"private"}}]})})});const result=await createGuestBookingDocument({repository:{transaction:async work=>work(unit)},clock,error})("management",{id:"doc"});expect(result).toMatchObject({hotel:"Issued Hotel",amount:123,dueAtIssue:77,entries:[{total:123}]});expect(JSON.stringify(result)).not.toContain("private");});
it("staff booking quotes retain the trimmed reason and injected time",async()=>{const app=createStaffBookingChange({clock,environment:async()=>"environment",quote:async(env,authority,kind,items,now)=>({env,authority,kind,items,now}),confirm:async()=>{throw new Error("unexpected confirmation");}});expect(await app(actor,"test",{action:"cancel-quote",reservationId:"booking",reason:"  Verified reason  ",items:[{stayId:"stay"}]},"key")).toMatchObject({env:"environment",kind:"cancel",now:100,authority:{actor,reason:"Verified reason",override:false}});});
it("staff booking confirmation requires a reviewed proposal token",async()=>{const app=createStaffBookingChange({clock,environment:async()=>"environment",quote:async()=>null,confirm:async()=>{throw new Error("unexpected confirmation");}});await expect(app(actor,"test",{action:"change-confirm",reservationId:"booking",reason:"Verified reason"},"key")).rejects.toThrow();});
const rollout=(r:Partial<PmsRolloutRepository>={},env="development")=>createPmsRollout({repository:async()=>repository<PmsRolloutRepository>(r),clock,require:()=>{},error,environment:()=>env,deployment:()=>"development",inventoryAccepted:()=>false});
it("development rollout route does not read or mutate release prerequisites",async()=>{await expect(rollout().assertProductionFeatureRoute(repository<PmsRolloutRepository>({}),"company","pms/command","POST")).resolves.toBeUndefined();});
it("production history reads remain accessible while rollout is off",async()=>{await expect(rollout({},"production").assertProductionFeatureRoute(repository<PmsRolloutRepository>({}),"company","pms/command","GET")).resolves.toBeUndefined();});
it("production activation denies a command when no current owner rollout is enabled",async()=>{const r=repository<PmsRolloutRepository>({engineering:async()=>false,enabled:async()=>[]});await expect(rollout({},"production").assertProductionFeatureRoute(r,"company","pms/command","POST")).rejects.toThrow("FEATURE_COMING_SOON");});
it("rollout mutation requires company scope before loading persistence",async()=>{await expect(rollout().changeCompanyFeatureRollout({...actor,branchId:"branch"},{},"key")).rejects.toThrow("FEATURE_COMPANY_REQUIRED");});
it("formal delivery acceptance requires all four retained evidence gates",()=>{for(const missing of ["automated","browser","provider","production"]as const)expect(productionAccepted({automated:true,browser:true,provider:true,production:true,[missing]:false})).toBe(false);expect(productionAccepted(undefined)).toBe(false);expect(productionAccepted({automated:true,browser:true,provider:true,production:true})).toBe(true);});
it("PMS source rules keep housekeeping away from guest account and refund money",()=>{const source: PmsSource={id:"source",property_id:"hotel",company_id:"company",branch_id:"branch",property_name:"Hotel",business_date:"2026-10-05",kind:"task",data:{},version:1,updated_at:100};const housekeeper={...actor,role:"pms_housekeeping"};expect(pmsSourceAllowed(housekeeper,source)).toBe(true);for(const kind of ["guest_account","refund","transfer"])expect(pmsSourceAllowed(housekeeper,{...source,kind})).toBe(false);});
it("unrelated outbox events never open a PMS projection transaction",async()=>{const app=createPmsSocialProjector({transaction:async()=>{throw new Error("unexpected transaction");},clock,hash:()=>"digest"});await expect(app.projectPmsSocialEvent(repository<EventMessage>({eventType:"other.event"}))).resolves.toBeUndefined();});
it("a retained PMS event receipt stops feed and notification writes",async()=>{const row:PmsSource={id:"reservation",property_id:"hotel",company_id:"company",branch_id:"branch",property_name:"Hotel",business_date:"2026-10-05",kind:"reservation",data:{state:"confirmed"},version:1,updated_at:100};const unit=repository<PmsSocialUnit>({original:async()=>({command_id:"command",created_at:100,actor_id:"creator",result_json:JSON.stringify({id:"reservation",propertyId:"hotel"})}),source:async()=>row,lock:async()=>null,received:async()=>({id:"receipt"})});const app=createPmsSocialProjector({transaction:async work=>work(unit),clock,hash:()=>"digest"});await expect(app.projectPmsSocialEvent(repository<EventMessage>({id:"event",eventType:"pms.reservation.create",aggregateId:"hotel",payload:{id:"hotel",version:1,companyId:"company",branchId:"branch",departmentId:null}}))).resolves.toBeUndefined();});
it("PMS import permission withdrawal prevents preview validation and unit-of-work acquisition",async()=>{const app=createPmsImport<never>({clock,hash:()=>"digest",require:()=>{throw error(403,"REVOKED");},error,isBusinessError:(_e):_e is Error&{code:string}=>{void _e;return false;}});await expect(app.previewPmsImport(repository<PmsImportRepository<never>>({}),actor,{},"key")).rejects.toThrow("REVOKED");});
it("PMS refund approval checks operational permission before querying the payment",async()=>{const permissions:string[]=[];const ports=repository<PmsBookingRefundPorts>({clock,require:(_actor,permission)=>{permissions.push(permission);throw error(403,"REVOKED");},error});const app=createPmsBookingRefund(ports);await expect(app.approveBookingRefund(repository<PmsBookingRefundRepository>({}),{...actor,branchId:"branch"},"payment")).rejects.toThrow("REVOKED");expect(permissions).toEqual(["pms.approve"]);});
it("Finance receipt recovery requires an owner reason before consulting Stripe",async()=>{const app=createPmsBookingRefund(repository<PmsBookingRefundPorts>({clock,require:()=>{},error}));await expect(app.recoverBookingReceipt(repository<PmsBookingRefundRepository>({}),actor,"refund","no")).rejects.toThrow("PMS_RECEIPT_RECOVERY_INPUT");});
const main=(overrides:Partial<Parameters<typeof createPmsApplication<never>>[0]>={})=>createPmsApplication<never>(repository<Parameters<typeof createPmsApplication<never>>[0]>({clock,uuid:()=>"00000000-0000-4000-8000-000000000001",hash:()=>"digest",require:()=>{},error,isBusinessError:(value):value is Error&{code:string}=>{void value;return false;},environment:()=>"development",...overrides}));
it("a PMS command cannot consume a partial list snapshot as authoritative state",async()=>{await expect(main().loadPms(repository<PmsUnit<never>>({}),actor,"hotel",true,{view:"dashboard"})).rejects.toThrow("PMS_COMMAND_REQUIRES_COMPLETE_STATE");});
it("PMS rejects restaurant imports outside the POS owner transaction before permission or repository access",async()=>{await expect(main({require:()=>{throw new Error("unexpected authorization");}}).pmsCommand(repository<PmsUnit<never>>({}),actor,{operation:"charge.import_pos",input:{}},"key")).rejects.toThrow("PMS_INTERNAL_COMMAND");});
it("production PMS requires verified bank evidence before acquiring the payment unit of work",async()=>{await expect(main({environment:()=>"production"}).pmsCommand(repository<PmsUnit<never>>({}),actor,{propertyId:"hotel",expectedVersion:1,operation:"payment.receive",input:{method:"transfer"}},"key")).rejects.toThrow("PMS_BANK_VERIFICATION_REQUIRED");});
it("PMS command permission withdrawal prevents transaction acquisition",async()=>{await expect(main({require:()=>{throw error(403,"REVOKED");}}).pmsCommand(repository<PmsUnit<never>>({}),actor,{propertyId:"hotel",expectedVersion:1,operation:"guest.save",input:{name:"Guest"}},"key")).rejects.toThrow("REVOKED");});
it("day-close advancement requires the worker boundary",async()=>{await expect(main().pmsCommand(repository<PmsUnit<never>>({}),actor,{propertyId:"hotel",expectedVersion:1,operation:"night.step",input:{}},"key")).rejects.toThrow("PMS_WORKER_ONLY");});
const versionApp=()=>main({error:(status,code,message,details)=>new ApiError(status,code,message,details)});
const versionRow=()=>({id:'hotel',company_id:'company',branch_id:'branch',name:'Hotel',business_date:'2026-10-05',state:'open' as const,config_json:JSON.stringify(snapshot().property.config),version:2});
for(const operation of ['guest.save','reservation.create','room.dnd','rate.save'])it(`stale ${operation} returns scoped locked version before loading command dependencies`,async()=>{
 const tx=repository<PmsUnit<never>>({assertProductionModuleEnabled:async()=>{},readContext:async()=>null,replay:async()=>null,readPmsProperties:async facts=>{expect(facts).toEqual({id:'hotel',companyId:'company',lock:true});return versionRow();}});
 const db=repository<PmsUnit<never>>({transaction:async work=>work(tx)});
 await expect(versionApp().pmsCommand(db,actor,{propertyId:'hotel',expectedVersion:1,operation,input:{id:'room',name:'Guest'}},'key')).rejects.toMatchObject({code:'PMS_VERSION_CONFLICT',details:{currentVersion:2}});
});
for(const operation of ['guest.save','reservation.create','room.dnd','rate.save'])it(`${operation} still requires expectedVersion before loading command dependencies`,async()=>{
 const tx=repository<PmsUnit<never>>({assertProductionModuleEnabled:async()=>{},readContext:async()=>null,replay:async()=>null,readPmsProperties:async()=>versionRow()});
 await expect(versionApp().pmsCommand(repository<PmsUnit<never>>({transaction:async work=>work(tx)}),actor,{propertyId:'hotel',operation,input:{id:'room',name:'Guest'}},'key')).rejects.toMatchObject({code:'PMS_VERSION_CONFLICT',details:{currentVersion:2}});
});
it('a stale command reveals no version across a Branch boundary',async()=>{
 const tx=repository<PmsUnit<never>>({assertProductionModuleEnabled:async()=>{},readContext:async()=>null,replay:async()=>null,readPmsProperties:async()=>versionRow()});
 await expect(versionApp().pmsCommand(repository<PmsUnit<never>>({transaction:async work=>work(tx)}),{...actor,branchId:'other'},{propertyId:'hotel',expectedVersion:1,operation:'guest.save',input:{name:'Guest'}},'key')).rejects.toMatchObject({code:'PMS_NOT_FOUND',details:undefined});
});
it('HTTP only exposes a safe scoped PMS conflict version',async()=>{
 expect(await errorResponse(new ApiError(409,'PMS_VERSION_CONFLICT','Review',{currentVersion:2})).json()).toEqual({error:{code:'PMS_VERSION_CONFLICT',message:'Review',details:{currentVersion:2}}});
 for(const [code,version] of [['PMS_NOT_FOUND',2],['PMS_VERSION_CONFLICT',0],['PMS_VERSION_CONFLICT',NaN]] as const)
  expect((await errorResponse(new ApiError(409,code,'Review',{currentVersion:version})).json()).error).not.toHaveProperty('details');
});
for(const [databaseCode,expected] of [['40001','PMS_RETRYABLE_CONFLICT'],['40P01','PMS_RETRYABLE_CONFLICT'],['23505','PMS_CONFLICT'],['23514','PMS_CONFLICT']] as const)
 it(`PMS distinguishes aborted ${databaseCode} from constraint conflicts`,async()=>{
  const db=repository<PmsUnit<never>>({transaction:async()=>{throw Object.assign(new Error('database fixture'),{code:databaseCode});}});
  await expect(versionApp().pmsCommand(db,actor,{propertyId:'hotel',expectedVersion:1,operation:'guest.save',input:{name:'Guest'}},'key')).rejects.toMatchObject({status:409,code:expected});
 });
const bookingPorts=(overrides:Partial<Parameters<typeof createPmsBooking<never>>[0]>={})=>repository<Parameters<typeof createPmsBooking<never>>[0]>({clock,uuid:()=>"00000000-0000-4000-8000-000000000001",hash:()=>"digest",error,require:()=>{},isBusinessError:(value):value is Error&{code:string}=>{void value;return false;},liveEnabled:()=>undefined,...overrides});
it("booking access rejects oversized private tokens before hashing or owner reads",async()=>{await expect(createPmsBooking(bookingPorts()).authorizedBooking(repository<PmsBookingUnit<never>>({}),"x".repeat(201),100)).rejects.toThrow("PMS_LINK_EXPIRED");});
it("payment access rejects invalid signatures before consulting the provider or receipt",async()=>{const app=createPmsBooking(bookingPorts({sign:()=>"expected",same:(a,b)=>a===b}));await expect(app.bookingPaymentStatus(repository<PmsBookingRepository<never>>({}),"payment.invalid",{secret:"fixture",origin:"https://fictional.example.test"},100)).rejects.toThrow("PMS_PAYMENT_TOKEN");});
it("booking recovery gives the same non-enumerating response when no guest matches",async()=>{const unit=repository<PmsBookingUnit<never>>({readPmsRecords3:async()=>[]}),repo=repository<PmsBookingRepository<never>>({transaction:async work=>work(unit)});expect(await createPmsBooking(bookingPorts()).recoverBooking(repo,"test",{number:"fictional",email:"guest@example.test"},{secret:"fixture",origin:"https://fictional.example.test"},100)).toEqual({message:"If these details match a booking, an access link will be emailed to you"});});
it("disabled booking mail does not lease messages or prepare provider delivery",async()=>{await expect(createPmsBooking(bookingPorts()).deliverBookingEmails(repository<PmsBookingRepository<never>>({}),{secret:"fixture",origin:"https://fictional.example.test"},fetch,100)).resolves.toBeUndefined();});
it("booking throttling binds the injected clock bucket and conceals the original key",async()=>{const unit=repository<PmsBookingUnit<never>>({readPmsBookingLimits1:async facts=>{expect(facts).toEqual({fact0:"private-digest",bucket:2});return {count:31};}});await expect(createPmsBooking(bookingPorts({clock:{now:()=>120001},hash:value=>{expect(value).toBe("sensitive-client-key");return "private-digest";}})).bookingRateLimit(unit,"sensitive-client-key")).rejects.toThrow("PMS_RATE_LIMIT");});
it("staff booking permission withdrawal happens before reading Holding facts",async()=>{await expect(createPmsBooking(bookingPorts({require:()=>{throw error(403,"REVOKED");}})).staffBookingSearch(actor,"hotel","availability",{})).rejects.toThrow("REVOKED");});

it('release timing diagnostics require an exact owned local cohort and cannot enable installed services',()=>{
 const directory='/workspace/.runtime/browser/abcdef123456';
 const env={NODE_ENV:'production',ERPX_PMS_RELEASE_DIAGNOSTICS:'1',ERPX_PERF_LOG:'1',ERPX_ENVIRONMENT:'local',ERPX_BROWSER_SIMULATION:'thai-hotel-v1',ERPX_BROWSER_SIMULATION_RUN_ID:'abcdef123456',ERPX_BROWSER_SIMULATION_PORT:'3104',ERPX_PUBLIC_ORIGIN:'http://127.0.0.1:3104',ERPX_DATA_DIR:directory,ERPX_DATABASE_MANIFEST:directory+'/databases.json'};
 expect(ownedReleaseReadDiagnostics(env)).toBe(true);
 for(const key of Object.keys(env))expect(ownedReleaseReadDiagnostics({...env,[key]:undefined})).toBe(false);
 for(const patch of [{ERPX_BROWSER_SIMULATION_PORT:'3101'},{ERPX_ENVIRONMENT:'production'},{ERPX_PUBLIC_ORIGIN:'https://console-dev-erpx.jesadakorn.com'},{ERPX_DATABASE_MANIFEST:'/production/databases.json'},{ERPX_BROWSER_SIMULATION_RUN_ID:'different'}])expect(ownedReleaseReadDiagnostics({...env,...patch})).toBe(false);
});


for(const operation of ['payment.receive','refund.pay','shift.open'])it(`stale ${operation} keeps the Property lock/version check before cashier dependencies`,async()=>{
 const tx=repository<PmsUnit<never>>({assertProductionModuleEnabled:async()=>{},readContext:async()=>null,replay:async()=>null,readPmsProperties:async facts=>{expect(facts.lock).toBe(true);return versionRow();}});
 await expect(versionApp().pmsCommand(repository<PmsUnit<never>>({transaction:async work=>work(tx)}),actor,{propertyId:'hotel',expectedVersion:1,operation,input:{method:'cash'}},'key')).rejects.toMatchObject({code:'PMS_VERSION_CONFLICT',details:{currentVersion:2}});
});

for(const operation of ['stay.check_in','stay.move','stay.check_out','reservation.cancel'])it(`stale ${operation} preserves the locked scoped version gate before operational dependencies`,async()=>{
 const tx=repository<PmsUnit<never>>({assertProductionModuleEnabled:async()=>{},readContext:async()=>null,replay:async()=>null,readPmsProperties:async facts=>{expect(facts).toEqual({id:'hotel',companyId:'company',lock:true});return versionRow();}});
 await expect(versionApp().pmsCommand(repository<PmsUnit<never>>({transaction:async work=>work(tx)}),actor,{propertyId:'hotel',expectedVersion:1,operation,input:{id:'stay',roomId:'room'}},'key')).rejects.toMatchObject({code:'PMS_VERSION_CONFLICT',details:{currentVersion:2}});
});
for(const operation of ['night.start','night.resume','guest_account.close'])it(`stale ${operation} checks the scoped Property version before close dependencies`,async()=>{
 const tx=repository<PmsUnit<never>>({assertProductionModuleEnabled:async()=>{},readContext:async()=>null,replay:async()=>null,readPmsProperties:async facts=>{expect(facts).toEqual({id:'hotel',companyId:'company',lock:true});return versionRow();}});
 await expect(versionApp().pmsCommand(repository<PmsUnit<never>>({transaction:async work=>work(tx)}),actor,{propertyId:'hotel',expectedVersion:1,operation,input:{id:'account'}},'key')).rejects.toMatchObject({code:'PMS_VERSION_CONFLICT',details:{currentVersion:2}});
});

for(const operation of ['charge','reverse'] as const)it(`stale POS ${operation} checks scoped locked Property version before money dependencies`,async()=>{
 const tx=repository<PmsUnit<never>>({assertProductionModuleEnabled:async()=>{},readPmsProperties:async facts=>{expect(facts).toEqual({id:'hotel',companyId:'company',lock:true});return versionRow();}});
 const promise=operation==='charge'?versionApp().applyPosRoomCharge(tx,actor,{source:'pos',propertyId:'hotel',companyId:'company',branchId:'branch',stayId:'stay',sourceId:'sale',sourceLineId:'line',currency:'THB',revenueOwner:'pms',sourceJournalId:null,description:'Cafe',net:100,tax:7,service:0,total:107},1):versionApp().reversePosRoomCharge(tx,actor,{propertyId:'hotel',sourceKey:'pos:sale:line',expectedPropertyVersion:1,reason:'Wrong item',total:107});
 await expect(promise).rejects.toMatchObject({code:'PMS_VERSION_CONFLICT',details:{currentVersion:2}});
});

import { availability, groupPickupCount, stayTypeOnDate } from "../../../src/lib/pms/pms-inventory-domain";
import { record, records, type DataMap, type Kind, type Snapshot } from "../../../src/server/modules/pms/public/rules/pms";
it("inactive Hotel controls retain frozen-mail retry without permitting new booking or settlement",()=>{
 const purchases={"pms.reservations":{newWork:false,settleExisting:false,history:true}};
 expect(pmsPackageCommandAllowed(purchases,"booking.retry_email",{id:"existing-failed-mail"})).toBe(true);
 expect(pmsPackageCommandAllowed(purchases,"booking.save",{})).toBe(false);
 expect(pmsPackageCommandAllowed(purchases,"reservation.cancel",{id:"existing-stay"})).toBe(false);
 expect(pmsPackageCommandAllowed({"pms.reservations":{newWork:false,settleExisting:false,history:false}},"booking.retry_email",{id:"existing-failed-mail"})).toBe(false);
});
function upgradeGroupFixture() {
  const s = newProperty('hotel', 'company', 'branch', 'Mock Hotel', '2026-09-01');
  s.property.state = 'open';
  let sequence = 0;
  const actor = { userId: 'desk', now: 100, uuid: () => `generated-${++sequence}`, independent: () => true };
  const add = <K extends Kind>(kind: K, id: string, data: DataMap[K]) => s.records.push({ kind, id, data, version: 1, updatedAt: 1 });
  for (const [typeId, price] of [['standard', 10000], ['suite', 20000]] as const) {
    add('room_type', typeId, { name: typeId, capacity: 4, amenities: '' });
    add('room', `${typeId}-room`, { number: typeId, typeId, building: 'A', floor: '1', active: true, cleanliness: 'inspected', dnd: false, observedOccupied: null });
    add('rate', `${typeId}-rate`, { name: typeId, typeId, from: '2026-09-01', to: '2026-09-30', price, extraAdult: 0, includedGuests: 2, minNights: 1, maxNights: 30, stopSell: false, closedArrival: false, closedDeparture: false, market: 'public', partyId: '', package: '', active: true });
  }
  add('guest', 'guest', { name: 'Mock Guest', email: '', phone: '', nationality: '', documentRef: '', notes: '' });
  const run = (op: string, input: unknown) => executePms(s, op, input, actor);
  const book = (arrival = '2026-09-01', departure = '2026-09-04', blockId = '') => {
    const id = run('reservation.create', { guestId: 'guest', source: 'phone', blockId, rooms: [{ typeId: 'standard', rateId: 'standard-rate', arrival, departure, adults: 2, quantity: 1 }] }).id;
    return records(s, 'stay').find(stay => stay.data.reservationId === id)!;
  };
  const upgrade = (id: string) => run('stay.upgrade', { id, roomId: 'suite-room', rateId: 'suite-rate', reason: 'Guest accepted suite rate' });
  const block = (to = '2026-09-06') => run('block.create', { name: 'Mock Group', typeId: 'standard', from: '2026-09-03', to, releaseDate: '2026-09-02', quantity: 1 }).id;
  const checkIn = (id: string) => { run('stay.assign', { id, roomId: 'standard-room' }); run('stay.check_in', { id }); };
  return { s, run, book, upgrade, block, checkIn };
}

it('upgrades only remaining nights and retains past prices, type occupancy and original money records', () => {
  const { s, book, checkIn, upgrade } = upgradeGroupFixture(), stay = book(); checkIn(stay.id);
  const past = structuredClone(stay.data.prices[0]);
  const money = JSON.stringify(s.records.filter(row => ['guest_account', 'entry'].includes(row.kind)));
  s.property.businessDate = '2026-09-02'; upgrade(stay.id);
  expect(stay.data.prices).toEqual([past, { date: '2026-09-02', net: 20000, service: 0, tax: 0, total: 20000 }, { date: '2026-09-03', net: 20000, service: 0, tax: 0, total: 20000 }]);
  expect(stayTypeOnDate(stay.data, '2026-09-01')).toBe('standard');
  expect(availability(s, 'standard', '2026-09-01').occupied).toBe(1);
  expect(availability(s, 'standard', '2026-09-02').occupied).toBe(0);
  expect(availability(s, 'suite', '2026-09-02').occupied).toBe(1);
  expect(JSON.stringify(s.records.filter(row => ['guest_account', 'entry'].includes(row.kind)))).toBe(money);
  expect(record(s, 'room', 'standard-room').data).toMatchObject({ cleanliness: 'dirty', cleaningCycle: 1 });
  expect(records(s, 'task')).toHaveLength(1);
  expect(stay.data.upgrades?.[0]).toMatchObject({ effectiveDate: '2026-09-02', actorId: 'desk', fromTypeId: 'standard', toTypeId: 'suite' });
});

it('repeated upgrades on one business date retain both decisions without zero-length inventory periods', () => {
  const { s, run, book, checkIn, upgrade } = upgradeGroupFixture(), stay = book(); checkIn(stay.id); upgrade(stay.id);
  record(s, 'room', 'standard-room').data.cleanliness = 'inspected';
  run('stay.upgrade', { id: stay.id, roomId: 'standard-room', rateId: 'standard-rate', reason: 'Guest returns to standard' });
  expect(stay.data.upgrades).toHaveLength(2);
  expect(stay.data.roomTypePeriods).toEqual([{ typeId: 'standard', from: '2026-09-01', to: '2026-09-04' }]);
  expect(availability(s, 'suite', '2026-09-01').occupied).toBe(0);
});

it('upgrade rejects uninspected rooms, incompatible rates, and issued destination bills before changing the stay', () => {
  const { s, book, checkIn, upgrade } = upgradeGroupFixture(), stay = book(); checkIn(stay.id);
  const before = JSON.stringify(stay);
  record(s, 'room', 'suite-room').data.cleanliness = 'dirty';
  expect(() => upgrade(stay.id)).toThrow('Room has not passed inspection');
  record(s, 'room', 'suite-room').data.cleanliness = 'inspected';
  record(s, 'rate', 'suite-rate').data.typeId = 'standard';
  expect(() => upgrade(stay.id)).toThrow('Rate is unavailable');
  record(s, 'rate', 'suite-rate').data.typeId = 'suite';
  record(s, 'guest_account', stay.data.guestAccountId).data.invoiceId = 'issued';
  expect(() => upgrade(stay.id)).toThrow('unissued bill');
  expect(JSON.stringify(stay)).toBe(before);
});

it('extends upgraded stays without reassigning past room type or nightly price', () => {
  const { s, run, book, checkIn, upgrade } = upgradeGroupFixture(), stay = book(); checkIn(stay.id);
  s.property.businessDate = '2026-09-02'; upgrade(stay.id);
  run('stay.change_dates', { id: stay.id, arrival: stay.data.arrival, departure: '2026-09-05', rateId: 'suite-rate', reason: 'Extend suite' });
  expect(stay.data.roomTypePeriods).toEqual([{ typeId: 'standard', from: '2026-09-01', to: '2026-09-02' }, { typeId: 'suite', from: '2026-09-02', to: '2026-09-05' }]);
  expect(stay.data.prices[0].total).toBe(10000);
});

it('reuses cancelled group quota while preserving both historical pickup IDs', () => {
  const { s, run, book, block } = upgradeGroupFixture(), blockId = block(), first = book('2026-09-03', '2026-09-05', blockId);
  run('stay.cancel', { id: first.id, reason: 'Cancelled' });
  expect(availability(s, 'standard', '2026-09-03')).toMatchObject({ occupied: 0, held: 1 });
  const second = book('2026-09-03', '2026-09-05', blockId);
  expect(record(s, 'block', blockId).data.pickedStayIds).toEqual([first.id, second.id]);
  expect(groupPickupCount(s, blockId, '2026-09-03')).toBe(1);
});

it('supports back-to-back group pickups but rejects overlapping use of the same quota', () => {
  const { s, book, block } = upgradeGroupFixture(), blockId = block();
  s.records.push({ ...structuredClone(record(s, 'room', 'standard-room')), id: 'standard-spare-room' });
  book('2026-09-03', '2026-09-04', blockId); book('2026-09-04', '2026-09-05', blockId);
  expect(record(s, 'block', blockId).data.pickedStayIds).toHaveLength(2);
  // Engine failures are rolled back by the application transaction; use a trial snapshot here.
  const trial: Snapshot = structuredClone(s);
  expect(() => executePms(trial, 'reservation.create', { guestId: 'guest', source: 'phone', blockId, rooms: [{ typeId: 'standard', rateId: 'standard-rate', arrival: '2026-09-03', departure: '2026-09-05', adults: 2 }] }, { userId: 'desk', now: 100, uuid: () => crypto.randomUUID(), independent: () => true })).toThrow('quota is unavailable');
});

it('changes pickup dates into its own remaining allotment without double-counting held capacity', () => {
  const { s, run, book, block } = upgradeGroupFixture(), blockId = block(), stay = book('2026-09-03', '2026-09-04', blockId);
  run('stay.change_dates', { id: stay.id, arrival: '2026-09-04', departure: '2026-09-06', rateId: 'standard-rate', reason: 'Shift pickup' });
  expect(availability(s, 'standard', '2026-09-03')).toMatchObject({ occupied: 0, held: 1 });
  expect(availability(s, 'standard', '2026-09-04')).toMatchObject({ occupied: 1, held: 0 });
});

it('returns remaining source-type group quota after upgrade and consumes target capacity exactly once', () => {
  const { s, run, book, block, checkIn, upgrade } = upgradeGroupFixture(), blockId = block(), stay = book('2026-09-03', '2026-09-05', blockId);
  run('stay.change_dates', { id: stay.id, arrival: '2026-09-01', departure: '2026-09-05', rateId: 'standard-rate', reason: 'Early group arrival' });
  checkIn(stay.id); upgrade(stay.id);
  expect(record(s, 'block', blockId).data.pickedStayIds).toEqual([stay.id]);
  expect(availability(s, 'standard', '2026-09-03')).toMatchObject({ occupied: 0, held: 1, available: 0 });
  expect(availability(s, 'suite', '2026-09-03')).toMatchObject({ occupied: 1, held: 0, available: 0 });
});

it('counts durable pending group claims once and returns the allotment when the hold expires', () => {
  const { s, book, block } = upgradeGroupFixture(), blockId = block(), stay = book('2026-09-03', '2026-09-04', blockId);
  s.asOf = 100;
  s.records.push({ id: 'pending-hold', kind: 'block', version: 1, updatedAt: 100, data: {
    name: 'Pending revision', typeId: 'standard', roomId: '', from: '2026-09-04', to: '2026-09-05', quantity: 1,
    releaseDate: '2026-09-30', state: 'active', kind: 'group', pickedStayIds: [],
    bookingChangeId: 'change', expiresAt: 200, groupPickupClaims: [{ blockId, stayId: stay.id }, { blockId, stayId: stay.id }],
  } });
  expect(groupPickupCount(s, blockId, '2026-09-04')).toBe(1);
  expect(availability(s, 'standard', '2026-09-04')).toMatchObject({ occupied: 0, held: 1, available: 0 });
  s.asOf = 200;
  expect(groupPickupCount(s, blockId, '2026-09-04')).toBe(0);
  expect(availability(s, 'standard', '2026-09-04')).toMatchObject({ occupied: 0, held: 1, available: 0 });
});

it('keeps a quota-only revision claim from consuming physical room capacity twice', () => {
  const { s, book, block } = upgradeGroupFixture(), blockId = block(), stay = book('2026-09-03', '2026-09-05', blockId);
  s.asOf = 100;
  s.records.push({ id: 'quota-marker', kind: 'block', version: 1, updatedAt: 100, data: {
    name: 'Pending same-night revision', typeId: 'standard', roomId: '', from: '2026-09-04', to: '2026-09-05', quantity: 1,
    releaseDate: '2026-09-30', state: 'active', kind: 'group', pickedStayIds: [], quotaOnly: true,
    bookingChangeId: 'change', expiresAt: 200, groupPickupClaims: [{ blockId, stayId: stay.id }],
  } });
  expect(groupPickupCount(s, blockId, '2026-09-04')).toBe(1);
  expect(availability(s, 'standard', '2026-09-04')).toMatchObject({ occupied: 1, held: 0, available: 0 });
  record(s, 'block', 'quota-marker').data.state = 'released';
  expect(availability(s, 'standard', '2026-09-04')).toMatchObject({ occupied: 1, held: 0, available: 0 });
});

import {createPmsHomeApi,type PmsHomeApiRepository} from '../../../src/server/modules/pms/application/home-api';
function handoverApi(r:Partial<PmsHomeApiRepository>){return createPmsHomeApi({repository:async()=>repository<PmsHomeApiRepository>(r),require:()=>{},error,postDraft:()=>{throw Error('unused');},mentions:()=>[],pmsHotelSources:async()=>{throw Error('unused');},pmsWorkspace:async()=>{throw Error('unused');},pmsHome:async()=>{throw Error('unused');}});}
it('PMS keeps headerless handover compatibility while the versioned path uses exactly the originally displayed version and key after native scope authorization',async()=>{
 const calls:unknown[][]=[],post:Awaited<ReturnType<PmsHomeApiRepository['readablePost']>>={id:'post',kind:'team',company_id:actor.companyId,branch_id:null,department_id:null,author_id:'author',author_name:'Author',body:'Actual read handover',mentions_json:'[]',content_json:null,rich_content_json:null,audience_kind:'participants',audience_id:null,purpose:'handover',edited_at:null,retracted_at:null,version:4,state:'published',linked_module:null,created_at:1},app=handoverApi({readablePost:async()=>{calls.push(['scope']);return post;},acknowledgeHandover:async(...args)=>{calls.push(['legacy',...args]);return {postId:'post',postVersion:4,acknowledged:true,canAcknowledge:true,acknowledgedCount:1,recipientCount:1};},acknowledgeHandoverVersion:async(...args)=>{calls.push(['versioned',...args]);return {postId:'post',postVersion:3,acknowledged:true};}});
 expect(await app.pmsHomeWrite(actor,['handover','post','acknowledgement'],{},'legacy-key')).toMatchObject({postVersion:4});expect(await app.pmsHomeWrite(actor,['handover','post','acknowledgement'],{},'original-key',3)).toEqual({postId:'post',postVersion:3,acknowledged:true});expect(calls).toEqual([['scope'],['legacy',actor,'post','legacy-key'],['scope'],['versioned',actor,'post','original-key',3]]);
});
it('PMS current native post scope failure precedes every handover confirmation including exact version retry',async()=>{
 const app=handoverApi({readablePost:async()=>{throw Error('POST_NOT_FOUND');}});await expect(app.pmsHomeWrite(actor,['handover','post','acknowledgement'],{},'original-key',3)).rejects.toThrow('POST_NOT_FOUND');
});

// Real Next request contexts and React render the original Today links and
// drawer caller. Only the drawer's asynchronous owner-read boundary is observed.
import {createElement,type ReactNode} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {PathnameContext,SearchParamsContext} from 'next/dist/shared/lib/hooks-client-context.shared-runtime';
import {StandardPmsToday as PmsToday} from '../../../src/components/pms/standard-pms-today';
import {PmsHotelSources} from '../../../src/components/records/pms-hotel-sources';
import {CollectionRecordLink} from '../../../src/components/records/record-collection';
import {CurrencyProvider} from '../../../src/components/finance/money/currency-provider';
import type {PmsHome} from '../../../src/server/modules/pms/public/pms/home';
const hotelDrawer=vi.hoisted(()=>({reads:[] as Array<{kind:string;endpoint:string}>}));
vi.mock('../../../src/components/records/record-collection',async importOriginal=>{
 const actual=await importOriginal<typeof import('../../../src/components/records/record-collection')>();
 return {...actual,RecordCollectionDrawer:(props:Parameters<typeof actual.RecordCollectionDrawer>[0])=>{hotelDrawer.reads.push({kind:props.kind,endpoint:props.endpoint});return null;}};
});
const sourceHotel:PmsHome['hotels'][number]={id:'original-hotel',name:'Original source hotel',branchId:'branch',businessDate:'2026-10-16',state:'open',currency:'THB',capacity:10,sold:2,inspected:8,roomCount:10,arrivals:1,departures:0,occupancy:0.2,roomRevenue:20000,adr:10000,revpar:2000,receivable:0,advance:0,deposit:0};
const sourceHome:PmsHome={updatedAt:100,hotels:[sourceHotel],tasks:{items:[],nextCursor:null,previousCursor:null,hasMore:false},counts:[],posts:[],readablePostIds:[],canFinance:false,feedHasMore:false,people:[],userId:actor.userId,view:'company'};
const sourceApi='/api/v1/holdings/test/pms-home',sourceFront='/test/original-company/holding/';
function renderHotelRoute(href:string,children:ReactNode){
 const url=new URL(href,'https://pms-dev-erpx.jesadakorn.com');
 return renderToStaticMarkup(createElement(PathnameContext,{value:url.pathname},createElement(SearchParamsContext,{value:url.searchParams},children)));
}
function metricLink(metric:string){
 const money:Parameters<typeof CurrencyProvider>[0]={initial:{companyId:actor.companyId,defaultCurrency:'THB',displayCurrency:'THB',scales:{THB:2},rates:[],currencies:[],canConfirmRate:false},apiBase:'/api/v1/holdings/test',children:createElement(PmsToday,{initial:sourceHome,api:sourceApi,frontHref:sourceFront,erpBase:'/holdings/test',branchId:null})};
 const html=renderHotelRoute('/holdings/test/pms/dashboard?tag=one&tag=two',createElement(CurrencyProvider,money));
 const hrefs=[...html.matchAll(/href="([^"]+)"/g)].map(match=>match[1].replaceAll('&amp;','&'));
 const href=hrefs.find(value=>new URL(value,'https://test.invalid').searchParams.get('metric')===metric);
 expect(href,'The original Today metric must render its owner link.').toBeDefined();
 return href!;
}
function hotelEndpoint(href:string,kind:string){
 hotelDrawer.reads.length=0;renderHotelRoute(href,createElement(PmsHotelSources,{api:sourceApi,frontHref:sourceFront}));
 const read=hotelDrawer.reads.find(item=>item.kind===kind);expect(read).toBeDefined();return new URL(read!.endpoint,'https://test.invalid');
}
function hotelReadApi(overrides:Partial<Parameters<typeof createPmsHomeApi>[0]>){return createPmsHomeApi({repository:async()=>repository<PmsHomeApiRepository>({}),require:()=>{},error,postDraft:()=>{throw Error('unused');},mentions:()=>[],pmsHotelSources:async()=>{throw Error('unused');},pmsWorkspace:async()=>{throw Error('unused');},pmsHome:async()=>{throw Error('unused');},...overrides});}
it.each(['occupancy','adr','revpar','room_revenue'] as const)('opens the actual Today %s source link with its original metric and business date through the real owner API',async metric=>{
 const href=metricLink(metric),url=hotelEndpoint(href,'pms_hotel_sources'),read=vi.fn(async()=>({hotel:sourceHotel,metric,rows:[],page:1,hasNext:false}));
 const result=await hotelReadApi({pmsHotelSources:read}).pmsHomeRead(actor,['hotel-sources',sourceHotel.id],url.searchParams);
 expect(result).toMatchObject({hotel:sourceHotel,metric});expect(read).toHaveBeenCalledExactlyOnceWith(actor,sourceHotel.id,metric,sourceHotel.businessDate,1);
 expect(new URL(href,'https://test.invalid').searchParams.getAll('tag')).toEqual(['one','two']);
});
it('opens an actual source-row link with its exact hotel and original record ID through the real detail owner API',async()=>{
 const link:Parameters<typeof CollectionRecordLink>[0]={kind:'pms_hotel_record',id:'entry:original',args:{property:sourceHotel.id},children:'Original room charge'};
 const html=renderHotelRoute(metricLink('adr'),createElement(CollectionRecordLink,link));
 const href=/href="([^"]+)"/.exec(html)?.[1].replaceAll('&amp;','&');expect(href).toBeDefined();
 const url=hotelEndpoint(href!,'pms_hotel_record'),read=vi.fn(async()=>{throw Error('OWNER_DETAIL_REACHED');});
 await expect(hotelReadApi({pmsWorkspace:read}).pmsHomeRead(actor,['hotel-record','entry:original'],url.searchParams)).rejects.toThrow('OWNER_DETAIL_REACHED');
 expect(read).toHaveBeenCalledExactlyOnceWith(actor,sourceHotel.id,false,true,{view:'detail',id:'entry:original'});
});
it('retains original JSON drawer arguments while canonical query fields select the current metric, date and hotel',()=>{
 const legacy=new URLSearchParams({detailKind:'pms_hotel_sources',detailId:sourceHotel.id,detailArgs:JSON.stringify({metric:'adr',businessDate:'2026-09-01',property:'old-hotel'}),detailPage:'2'});
 const old=hotelEndpoint('/holdings/test/pms/dashboard?'+legacy,'pms_hotel_sources');expect(Object.fromEntries(old.searchParams)).toEqual({metric:'adr',businessDate:'2026-09-01',page:'2'});
 legacy.set('metric','revpar');legacy.set('businessDate',sourceHotel.businessDate);legacy.set('property',sourceHotel.id);
 const current=hotelEndpoint('/holdings/test/pms/dashboard?'+legacy,'pms_hotel_sources');expect(Object.fromEntries(current.searchParams)).toEqual({metric:'revpar',businessDate:sourceHotel.businessDate,page:'2'});
 expect(hotelEndpoint('/holdings/test/pms/dashboard?'+legacy,'pms_hotel_record').searchParams.get('property')).toBe(sourceHotel.id);
});
it('keeps malformed canonical metric and date rejected by the original owner API without reaching source facts',async()=>{
 const read=vi.fn(async()=>{throw Error('unexpected owner read');}),app=hotelReadApi({pmsHotelSources:read});
 for(const query of [{metric:'unknown',businessDate:sourceHotel.businessDate},{metric:'adr',businessDate:'not-a-date'}]){
  const url=hotelEndpoint('/holdings/test/pms/dashboard?'+new URLSearchParams({...query,detailKind:'pms_hotel_sources',detailId:sourceHotel.id}),'pms_hotel_sources');
  await expect(app.pmsHomeRead(actor,['hotel-sources',sourceHotel.id],url.searchParams)).rejects.toThrow();
 }
 expect(read).not.toHaveBeenCalled();
});


it("the actual PMS worker uses Booking for newly reconciled guest mail while preserving isolated origins and disabled routing",async()=>{
 const {readFileSync}=await import('node:fs'),ts=await import('typescript'),{applicationOrigin}=await import('../../../src/lib/runtime/application-origins');
 const source=readFileSync('src/server/modules/runtime/adapters/runtime/jobs.ts','utf8'),file=ts.createSourceFile('jobs.ts',source,ts.ScriptTarget.Latest,true),declaration=file.statements.find((node):node is import('typescript').FunctionDeclaration=>ts.isFunctionDeclaration(node)&&node.name?.text==='pmsWork');
 if(!declaration)throw Error('The original PMS worker is required');
 const code=ts.transpileModule(declaration.getText(file).replace(/^export\s+/,''),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext}}).outputText.replace(/import\(/g,'modules(');
 const db={},payment=vi.fn(async()=>{}),mail=vi.fn(async()=>{}),drain=vi.fn(async()=>{}),stripe=vi.fn(async()=>{}),send=vi.fn();
 const modules=async(name:string)=>{
  if(name.endsWith('/pms-booking'))return {drainBookingPayments:payment,deliverBookingEmails:mail};
  if(name.endsWith('/simulation-email'))return {simulationBookingEmailProvider:()=>send};
  if(name.endsWith('/pms-stripe'))return {drainPmsStripePayments:stripe};
  if(name.endsWith('/pms'))return {drainPms:drain};
  throw Error('Unexpected worker module: '+name);
 };
 const run=new Function('activeSlots','registeredResources','applicationOrigin','modules',code+';return pmsWork;')(async()=>[{id:'H001',beta:false}],()=>({write:db}),applicationOrigin,modules) as (env:Record<string,unknown>)=>Promise<void>;
 const legacy='https://console-dev-erpx.jesadakorn.com';
 try{
  vi.stubEnv('ERPX_PUBLIC_ORIGIN',legacy);vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');vi.stubEnv('ERPX_PRODUCT_SSO','enabled');
  const base={BETTER_AUTH_SECRET:'isolated-worker-secret',RESEND_API_KEY:'isolated-mail-key',RESEND_FROM:'hotel@example.test'};
  for(const [origin,enabled,expected] of [[legacy,true,'https://booking-dev-erpx.jesadakorn.com'],['http://127.0.0.1:3104',true,'http://127.0.0.1:3104'],['https://independent.example.test',true,'https://independent.example.test'],[legacy,false,legacy]] as const){
   vi.stubEnv('ERPX_PRODUCT_ROUTING',enabled?'enabled':'disabled');payment.mockClear();mail.mockClear();
   await run({...base,ERPX_PUBLIC_ORIGIN:origin});
   const forwarded={secret:base.BETTER_AUTH_SECRET,origin:expected,apiKey:base.RESEND_API_KEY,from:base.RESEND_FROM};
   expect(payment).toHaveBeenCalledExactlyOnceWith(db,forwarded);expect(mail).toHaveBeenCalledExactlyOnceWith(db,forwarded,send);
  }
  expect(drain).toHaveBeenCalledTimes(4);expect(stripe).toHaveBeenCalledTimes(4);expect(send).not.toHaveBeenCalled();
 }finally{vi.unstubAllEnvs();}
});


it("delivers queued legacy Booking links on Booking without replacing their token, tenant, query or unrelated destinations",async()=>{
 const {bookingEmailDeliveryPayload}=await import('../../../src/server/modules/pms/adapters/booking-email-delivery');
 const old='https://console-dev-erpx.jesadakorn.com/book/tenant%20one/manage?lang=th#token=original%2Ftoken',other='https://outside.example.test/book/tenant/manage#token=foreign',input=JSON.stringify({from:'hotel@example.test',to:['guest@example.test'],subject:'Original confirmation',text:`Original policy\n${old}\n${other}\nExpires in seven days.`});
 try{
  vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://console-dev-erpx.jesadakorn.com');vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');vi.stubEnv('ERPX_PRODUCT_SSO','enabled');
  const env={secret:'isolated-secret',origin:'https://booking-dev-erpx.jesadakorn.com'},expected=JSON.parse(input);expected.text=expected.text.replace(old,old.replace('console-dev-erpx','booking-dev-erpx'));
  const delivered=bookingEmailDeliveryPayload(input,env);expect(JSON.parse(delivered)).toEqual(expected);expect(bookingEmailDeliveryPayload(delivered,env)).toBe(delivered);
  for(const origin of ['http://127.0.0.1:3104','https://independent.example.test'])expect(bookingEmailDeliveryPayload(input,{...env,origin})).toBe(input);
  for(const link of [other,'https://console-dev-erpx.jesadakorn.com/book/tenant/manage','https://console-dev-erpx.jesadakorn.com/book/tenant/hotel#token=original','https://console-dev-erpx.jesadakorn.com.evil.test/book/tenant/manage#token=foreign']){const unchanged=JSON.stringify({text:link});expect(bookingEmailDeliveryPayload(unchanged,env)).toBe(unchanged);}
  vi.stubEnv('ERPX_PRODUCT_ROUTING','disabled');expect(bookingEmailDeliveryPayload(input,env)).toBe(input);
 }finally{vi.unstubAllEnvs();}
});


// Native command paths, with original receipt IDs and no replacement money.
function correctedReceiptFixture(){
 const base=upgradeGroupFixture(),stay=base.book(),s=base.s;
 s.property.config.approvedBy='independent-finance';let sequence=1000,now=1000;
 const run=(operation:string,input:unknown,userId='desk')=>executePms(s,operation,input,{userId,now:++now,uuid:()=>`receipt-proof-${++sequence}`,independent:other=>other!==userId});
 const shift=run('shift.open',{name:'Original correction receipt',opening:0}).id;
 return {s,run,shift,bill:stay.data.guestAccountId};
}
it('a paid native charge correction releases only its consumed original receipt and never funds a second refund',async()=>{
 const {availableReceiptSources}=await import('../../../src/lib/pms/pms-receipt-funding');
 const {s,run,shift,bill}=correctedReceiptFixture(),charge=run('charge.add',{guestAccountId:bill,price:1000,description:'Original paid service'}).id,receipt=run('payment.receive',{guestAccountId:bill,amount:1000,method:'cash',shiftId:shift}).id;
 const original=JSON.stringify(records(s,'entry'));expect(availableReceiptSources(s,bill,'advance')).toEqual([]);
 run('charge.reverse',{id:charge,reason:'Independently corrected original service'},'reviewer');expect(availableReceiptSources(s,bill,'advance')).toEqual([{entryId:receipt,amount:1000}]);expect(JSON.stringify(records(s,'entry').slice(0,JSON.parse(original).length))).toBe(original);
 const refund=(amount:number)=>{const id=run('refund.request',{guestAccountId:bill,amount,pocket:'advance',reason:'Original corrected receipt'}).id;run('refund.approve',{id},'reviewer');return run('refund.pay',{id,method:'cash',shiftId:shift}).id;};
 const first=refund(200);expect(record(s,'entry',first).data.funding).toEqual([{entryId:receipt,amount:200}]);
 const next=run('charge.add',{guestAccountId:bill,price:300,description:'Actual later service'}).id;expect(availableReceiptSources(s,bill,'advance')).toEqual([{entryId:receipt,amount:500}]);run('charge.reverse',{id:next,reason:'Independent later correction'},'reviewer');
 const last=refund(800);expect(record(s,'entry',last).data.funding).toEqual([{entryId:receipt,amount:800}]);expect(availableReceiptSources(s,bill,'advance')).toEqual([]);expect(record(s,'guest_account',bill).data.advance).toBe(0);expect(()=>refund(1)).toThrow();
});
it('a partial paid correction releases only its actual credit, retaining unrelated consumed source and refund holds',async()=>{
 const {availableReceiptSources}=await import('../../../src/lib/pms/pms-receipt-funding');const {s,run,shift,bill}=correctedReceiptFixture();
 const corrected=run('charge.add',{guestAccountId:bill,price:1000,description:'Partially refundable service'}).id;run('charge.add',{guestAccountId:bill,price:1000,description:'Unchanged actual service'});const receipt=run('payment.receive',{guestAccountId:bill,amount:1500,method:'cash',shiftId:shift}).id;
 const outcome=run('charge.reverse',{id:corrected,reason:'Independent partial credit'},'reviewer');expect(outcome.effects.postings.find(row=>row.source===`${outcome.id}:credit-balance`)?.lines).toEqual([{account:'receivable',debit:500,credit:0},{account:'advance',debit:0,credit:500}]);expect(availableReceiptSources(s,bill,'advance')).toEqual([{entryId:receipt,amount:500}]);
 record(s,'guest_account',bill).data.refundHolds=[{entryId:receipt,amount:200,pocket:'advance'}];expect(availableReceiptSources(s,bill,'advance')).toEqual([{entryId:receipt,amount:300}]);expect(availableReceiptSources(s,bill,'advance',false)).toEqual([{entryId:receipt,amount:500}]);expect(availableReceiptSources(s,bill,'deposit')).toEqual([]);
});
it('a cloned legacy correction needs exact owned credit evidence and cannot release another bill or allocation twice',async()=>{
 const {availableReceiptSources,captureReceiptAllocationCredit}=await import('../../../src/lib/pms/pms-receipt-funding');const {s,run,shift,bill}=correctedReceiptFixture(),charge=run('charge.add',{guestAccountId:bill,price:1000,description:'Historical paid charge'}).id,receipt=run('payment.receive',{guestAccountId:bill,amount:1000,method:'cash',shiftId:shift}).id,correction=run('charge.reverse',{id:charge,reason:'Historical independently corrected charge'},'reviewer').id;
 const restored=structuredClone(s),before=JSON.stringify(restored);expect(availableReceiptSources(restored,bill,'advance')).toEqual([]);expect(availableReceiptSources(s,bill,'advance')).toEqual([{entryId:receipt,amount:1000}]);
 captureReceiptAllocationCredit(restored,correction,1000);captureReceiptAllocationCredit(restored,correction,1000);expect(availableReceiptSources(restored,bill,'advance')).toEqual([{entryId:receipt,amount:1000}]);expect(JSON.stringify(restored)).toBe(before);expect(()=>captureReceiptAllocationCredit(restored,correction,1001)).toThrow('PMS_RECEIPT_CREDIT_CHANGED');
 const wrongBill=structuredClone(s);record(wrongBill,'entry',correction).data.guestAccountId='other-bill';captureReceiptAllocationCredit(wrongBill,correction,1000);expect(availableReceiptSources(wrongBill,bill,'advance')).toEqual([]);expect(availableReceiptSources(wrongBill,'other-bill','advance')).toEqual([]);
 const wrongSource=structuredClone(s);record(wrongSource,'entry',charge).data.method='opening';captureReceiptAllocationCredit(wrongSource,correction,1000);expect(availableReceiptSources(wrongSource,bill,'advance')).toEqual([]);
 const excessive=structuredClone(s);captureReceiptAllocationCredit(excessive,correction,1001);expect(availableReceiptSources(excessive,bill,'advance')).toEqual([]);
});

for(const lock of [false,true])it(`the actual Booking ${lock?'locked refund':'read quote'} snapshot rehydrates only native correction proof without serializing it`,async()=>{
 const {availableReceiptSources}=await import('../../../src/lib/pms/pms-receipt-funding');const {s,run,shift,bill}=correctedReceiptFixture(),charge=run('charge.add',{guestAccountId:bill,price:1000,description:'Actual prior corrected charge'}).id,receipt=run('payment.receive',{guestAccountId:bill,amount:1000,method:'cash',shiftId:shift}).id,credit=run('charge.reverse',{id:charge,reason:'Prior independent correction'},'reviewer').id;
 const native=vi.fn(async(facts:Parameters<PmsUnit<never>['readReceiptAllocationCredits']>[0])=>{expect(facts).toEqual({companyId:s.property.companyId,propertyId:s.property.id,branchId:s.property.branchId,chartId:s.property.config.chartId,currency:s.property.config.currency,receivableAccountId:s.property.config.accounts.receivable,advanceAccountId:s.property.config.accounts.advance,entries:[{id:credit,date:s.property.businessDate}]});return [{entryId:credit,amount:1000}];});
 const owner=main(),db=repository<PmsBookingUnit<never>>({readPmsProperties1:async facts=>{expect(facts).toEqual({propertyId:s.property.id,lock});const p=s.property;return {id:p.id,company_id:p.companyId,branch_id:p.branchId,name:p.name,business_date:p.businessDate,state:p.state,version:p.version,config_json:JSON.stringify(p.config)};},readPmsRecords2:async()=>s.records.map(row=>({id:row.id,kind:row.kind,data_json:JSON.stringify(row.data),version:row.version,updated_at:row.updatedAt})),hydrateReceiptAllocationCredits:async snapshot=>owner.hydrateReceiptAllocationCredits(repository<PmsUnit<never>>({readReceiptAllocationCredits:native}),snapshot)});
 const loaded=await createPmsBooking(bookingPorts()).bookingSnapshot(db,s.property.id,undefined,lock);expect(native).toHaveBeenCalledOnce();expect(loaded).not.toBe(s);expect(availableReceiptSources(loaded,bill,'advance')).toEqual([{entryId:receipt,amount:1000}]);expect(availableReceiptSources(JSON.parse(JSON.stringify(loaded)),bill,'advance')).toEqual([]);expect(Object.keys(loaded)).toEqual(['property','records']);
});

import {StandardPmsHomeEditor} from '../../../src/components/pms/standard-pms-today';
import {DraftFormDrawer} from '../../../src/components/ui/draft-form-drawer';
import {Children,isValidElement,type ComponentProps} from 'react';
it.each([{busy:true,pending:false},{busy:false,pending:true}])('actual standard PMS Home editor retains its native draft/recovery and refuses dismissal for %j',state=>{
 const close=vi.fn(),fields=createElement('textarea',{defaultValue:'Newer unsent handover text'}),recovery=createElement('button',{type:'button'},'Retry original request');
 const node=StandardPmsHomeEditor({title:'Original handover',language:'en',open:true,dirty:true,...state,onClose:close,recovery,children:fields});
 if(!isValidElement<ComponentProps<typeof DraftFormDrawer>>(node))throw Error('The actual editor must use the standard right drawer.');
 expect(node.type).toBe(DraftFormDrawer);expect(node.props).toMatchObject({appearance:'standard',open:true,title:'Original handover',dirty:true,dismissDisabled:true,retainFormContents:true});expect(Children.toArray(node.props.children).map(child=>isValidElement(child)?child.type:null)).toEqual(['button','textarea']);
 node.props.onClose();node.props.onDiscard?.();expect(close).not.toHaveBeenCalled();expect(fields.props.defaultValue).toBe('Newer unsent handover text');expect(node.props.children).toEqual([recovery,fields]);
});
it.each(['th','en'] as const)('actual standard PMS Home %s editor can close a known idle draft while retaining its controlled values and honest publication copy',language=>{
 const close=vi.fn(),fields=createElement('textarea',{defaultValue:'Retained original text'}),node=StandardPmsHomeEditor({title:'Native post',language,open:true,dirty:true,busy:false,pending:false,onClose:close,recovery:null,children:fields});
 node.props.onClose();expect(close).toHaveBeenCalledOnce();expect(node.props.children).toEqual([null,fields]);expect(fields.props.defaultValue).toBe('Retained original text');expect(node.props.discardLabel).toBe(language==='th'?'ปิดโดยไม่เผยแพร่':'Close without publishing');expect(node.props.keepEditingLabel).toBe(language==='th'?'แก้ไขต่อ':'Keep editing');
});
it('actual standard PMS Today keeps native hotel sources and handover work selection outside its initially closed long editor',()=>{
 const network=vi.spyOn(globalThis,'fetch').mockImplementation(async()=>{throw Error('Standard PMS Today SSR must not perform an HTTP request.');});
 try{
  const money:Parameters<typeof CurrencyProvider>[0]={initial:{companyId:actor.companyId,defaultCurrency:'THB',displayCurrency:'THB',scales:{THB:2},rates:[],currencies:[],canConfirmRate:false},apiBase:'/api/v1/holdings/test',children:createElement(PmsToday,{initial:sourceHome,api:sourceApi,frontHref:sourceFront,erpBase:'/holdings/test',branchId:null})};
  const html=renderHotelRoute('/holdings/test/pms/dashboard?homeView=handover',createElement(CurrencyProvider,money));
  expect(html).toContain('ui-page-header');expect(html).toContain('data-appearance="standard"');expect(html).toContain('id="pms-work"');expect(html).toContain('id="pms-handover"');expect(html).toContain('Prepare handover');expect(html).toContain('Original source hotel');expect(html).not.toContain('name="recipientIds"');expect(network).not.toHaveBeenCalled();
 }finally{network.mockRestore();}
});
