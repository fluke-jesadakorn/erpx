import { ownedReleaseReadDiagnostics } from "../../src/server/runtime/read-request";
import {expect,it} from "vitest";
import {ApiError,errorResponse} from '../../src/server/http';
import {createPmsReportsApplication,type PmsReportsRepository} from "../../src/server/modules/pms/application/reports";
import {createGuestBookingDocument,type GuestDocumentUnit} from "../../src/server/modules/pms/application/guest-document";
import {createStaffBookingChange} from "../../src/server/modules/pms/application/staff-booking-changes";
import {createPmsRollout,type PmsRolloutRepository} from "../../src/server/modules/pms/application/hospitality-rollout";
import {createPmsSocialProjector,type PmsSocialUnit} from "../../src/server/modules/pms/application/social-events";
import {createPmsImport,type PmsImportRepository} from "../../src/server/modules/pms/application/import";
import {createPmsBookingRefund,type PmsBookingRefundPorts,type PmsBookingRefundRepository} from "../../src/server/modules/pms/application/pms-booking-refunds";
import {productionAccepted} from "../../src/server/modules/pms/application/hospitality-evidence";
import {pmsSourceAllowed,type PmsSource} from "../../src/server/modules/pms/domain/source";
import {newProperty,executePms} from "../../src/lib/pms-engine";
import {createPmsApplication,type PmsUnit} from "../../src/server/modules/pms/application/pms";
import {createPmsBooking,type PmsBookingUnit,type PmsBookingRepository} from "../../src/server/modules/pms/application/pms-booking";
import type {WorkContext} from "../../src/server/modules/identity_access/public/authorization";
import type {EventMessage} from "../../src/server/modules/workflow/public/job-projector";
function repository<T extends object>(operations:Partial<T>):T{return new Proxy(operations as T,{get(target,name){if(name==="then")return undefined;if(name in target)return Reflect.get(target,name);return ()=>{throw new Error(`unexpected operation: ${String(name)}`);};}});}
const error=(_status:number,code:string)=>new Error(code),clock={now:()=>100};
const actor:WorkContext={userId:"reviewer",name:"Reviewer",email:"reviewer@example.test",assignmentId:"assignment",role:"holding_owner",companyId:"company",branchId:null,departmentId:null,holdingId:"H001",holdingSlug:"test",holdingName:"Test",scope:{kind:"holding",holdingId:"H001",holdingSlug:"test",holdingName:"Test",companyId:"company"},assignmentScope:"holding",inherited:false,modules:["pms"],assignments:[],workspaces:[]};
const snapshot=()=>newProperty("hotel","company","branch","Hotel","2026-10-05");
const reports=createPmsReportsApplication({clock:{isoNow:()=>"2026-10-09T00:00:00.000Z"},require:()=>{},error});
const reportInput={propertyId:"hotel",from:"2026-10-01",to:"2026-10-02"};
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

import { availability, groupPickupCount, stayTypeOnDate } from "../../src/lib/pms-inventory-domain";
import { record, records, type DataMap, type Kind, type Snapshot } from "../../src/lib/pms";
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
