/** Real PMS/POS setup inside the disposable browser simulation databases only. */
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { PostgresDatabase } from "../../src/server/runtime/postgres";
import type { SqlDatabase } from "../../src/server/runtime/types";
import type { WorkContext } from "../../src/server/authorization";
import { businessRoles } from "../../src/lib/business-roles";
import { accountKeys } from "../../src/lib/pos";
import { pmsAccountKeys, plusDays, record, records, type Config } from "../../src/lib/pms";

type Fixture = {
  name:string;legalEntity:string;property:{rooms:{number:string;type:string;state:string}[];roomTypes:{code:string;name:string;capacity:number;rateMinor:number}[];taxAndService:{vatBpsExampleOnly:number;serviceBpsExampleOnly:number;taxInclusiveExampleOnly:boolean}};
  restaurant:{register:string;warehouse:string;tables:string[];menu:{sku:string;name:string;priceMinor:number;recipe?:{sku:string;quantityMilli:number}[];stockSku?:string}[];openingStock:{sku:string;unit:string;quantityMilli:number;valueMinor:number}[]};
  demoActors:{key:string;role:keyof typeof businessRoles|"holding_owner";email:string}[];
};
type DemoUser={label:string;email:string;role:string;userId:string;roles:string[]};

export async function seedHospitalityBrowserDemo(input:{db:SqlDatabase;control:SqlDatabase;files:Parameters<typeof import("../../fixtures/expenses/company").seedExpenseCompany>[1];passwordHash:string;run:string;origin:string}) {
  if(!(input.db instanceof PostgresDatabase))throw new Error("ISOLATED_POSTGRES_REQUIRED");
  const db=input.db;
  const fixture=JSON.parse(await readFile(resolve(import.meta.dir,"../../fixtures/hospitality/pilot-example.json"),"utf8")) as Fixture;
  const {seedExpenseCompany}=await import("../../fixtures/expenses/company");
  const {pmsCommand,loadPms}=await import("../../src/server/pms");
  const {posCommand}=await import("../../src/server/pos-core");
  const {saveMaster}=await import("../../src/server/commerce");
  const f=await seedExpenseCompany(input.db,input.files);
  await input.db.prepare("UPDATE organization_nodes SET display_name=? WHERE id=?").bind(fixture.legalEntity,f.company).run();
  await input.db.prepare("UPDATE organization_nodes SET display_name=? WHERE id=?").bind("Riverside Lantern Bangkok",f.branch).run();
  const now=Date.now(),actors:Record<string,WorkContext>={},users:DemoUser[]=[];
  for(const entry of fixture.demoActors) {
    const actor=entry.role==="holding_owner"?f.owner:await f.member(`riverside-${entry.key}`,entry.role);
    actor.holdingId="h002";actor.holdingSlug="riverside-lantern";actor.holdingName=fixture.name;actor.scope={kind:"holding",holdingId:"h002",holdingSlug:"riverside-lantern",holdingName:fixture.name,companyId:f.company};
    actor.email=entry.email;actor.name=entry.key.replace(/([A-Z])/g," $1").replace(/^./,c=>c.toUpperCase());
    const modules=entry.role==="holding_owner"?["pms","pos","finance","inventory","procurement","organization","iam"]:businessRoles[entry.role];
    actor.modules=[...modules];
    await input.db.prepare("UPDATE assignments SET modules_json=? WHERE id=?").bind(JSON.stringify(modules),actor.assignmentId).run();
    await input.db.prepare("UPDATE hr_people SET legal_name=?,preferred_name=?,email=? WHERE company_id=? AND user_id=?").bind(actor.name,actor.name,entry.email,f.company,actor.userId).run();
    await input.control.batch([
      input.control.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind(actor.userId,actor.name,entry.email,now,now),
      input.control.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)").bind(`account-${actor.userId}`,actor.userId,actor.userId,input.passwordHash,now,now),
      input.control.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)").bind(`principal-${actor.userId}`,actor.userId,actor.name,now),
    ]);
    actors[entry.key]=actor;users.push({label:actor.name,email:entry.email,role:entry.role,userId:actor.userId,roles:[entry.role]});
  }
  const admin=actors.administrator,finance=actors.financeApprover;
  const pmsAccounts={} as Config["accounts"];
  const allKeys=[...new Set<string>([...pmsAccountKeys,...accountKeys])];
  for(const [index,name] of allKeys.entries()) {
    const id=`${f.company}-hospitality-${name}`;
    const category=["revenue","service"].includes(name)?"revenue":["advance","deposit","tax","outputTax","refund","disputes"].includes(name)?"liability":["consumption","commission","cashDifference","cogs","fees","waste"].includes(name)?"expense":"asset";
    await input.db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',?,?)").bind(id,f.chart,f.company,`HOTEL-${index}`,`Riverside ${name}`,category,now,now).run();
    if((pmsAccountKeys as readonly string[]).includes(name))pmsAccounts[name as keyof Config["accounts"]]=id;
  }
  const posAccounts=Object.fromEntries(accountKeys.map(name=>[name,`${f.company}-hospitality-${name}`]));
  const date=new Intl.DateTimeFormat("en-CA",{timeZone:"Asia/Bangkok",year:"numeric",month:"2-digit",day:"2-digit"}).format(new Date());
  const nextYear=`${Number(date.slice(0,4))+1}-${date.slice(5)}`;
  const propertyId=String((await pmsCommand(input.db,admin,{operation:"property.create",input:{name:fixture.name,branchId:f.branch,businessDate:date}},crypto.randomUUID())).id);
  const pms=async(operation:string,details:unknown={},actor=admin)=>{
    const snapshot=await loadPms(input.db,admin,propertyId);
    return pmsCommand(db,actor,{propertyId,expectedVersion:snapshot.property.version,operation,input:details},crypto.randomUUID());
  };
  await pms("accounting.save",{chartId:f.chart,accounts:pmsAccounts,taxBps:fixture.property.taxAndService.vatBpsExampleOnly,serviceBps:fixture.property.taxAndService.serviceBpsExampleOnly,taxInclusive:fixture.property.taxAndService.taxInclusiveExampleOnly,advanceTax:true});
  await pms("accounting.approve",{},finance);
  const types:Record<string,string>={};
  for(const roomType of fixture.property.roomTypes)types[roomType.name]=String((await pms("room_type.save",{name:roomType.name,capacity:roomType.capacity,amenities:"Fictional demo room"})).id);
  const rooms:Record<string,string>={};
  for(const room of fixture.property.rooms)rooms[room.number]=String((await pms("room.save",{number:room.number,typeId:types[room.type],active:room.state!=="maintenance_blocked"})).id);
  const rates:Record<string,string>={};
  for(const roomType of fixture.property.roomTypes)rates[roomType.name]=String((await pms("rate.save",{name:`${roomType.name} flexible`,typeId:types[roomType.name],from:date,to:nextYear,price:roomType.rateMinor})).id);
  await pms("property.enable");
  for(const room of fixture.property.rooms.filter(room=>room.state==="inspected"||room.state==="occupied")) {
    const taskId=String((await pms("task.create",{roomId:rooms[room.number],type:"arrival",date})).id);
    const task=record(await loadPms(input.db,admin,propertyId),"task",taskId);
    await pms("task.complete",{id:taskId,checked:task.data.checklist},actors.housekeeper);
    await pms("task.inspect",{id:taskId},actors.inspector);
  }
  const occupied=fixture.property.rooms.find(room=>room.state==="occupied");
  if(occupied) {
    const guestId=String((await pms("guest.save",{name:"Existing Demo Guest"},actors.frontDesk)).id);
    const booking=await pms("reservation.create",{guestId,source:"staff",rooms:[{typeId:types[occupied.type],rateId:rates[occupied.type],arrival:date,departure:plusDays(date,2),adults:2,quantity:1}]},actors.frontDesk);
    const stay=records(await loadPms(db,admin,propertyId),"stay").find(item=>item.data.reservationId===booking.id);
    if(!stay)throw new Error("DEMO_OCCUPIED_STAY_MISSING");
    await pms("stay.assign",{id:stay.id,roomId:rooms[occupied.number]},actors.frontDesk);
    await pms("stay.check_in",{id:stay.id},actors.frontDesk);
  }
  const warehouseId=String((await saveMaster(admin,"warehouses",null,{branchId:f.branch,code:"RLH-KITCHEN",name:fixture.restaurant.warehouse},crypto.randomUUID())).id);
  const products:Record<string,string>={};
  for(const stock of fixture.restaurant.openingStock) {
    const menuItem=fixture.restaurant.menu.find(item=>item.stockSku===stock.sku);
    products[stock.sku]=String((await saveMaster(admin,"products",null,{sku:`RLH-${stock.sku}`,name:menuItem?.name??stock.sku,unit:stock.unit,price:String((menuItem?.priceMinor??100)/100),taxBps:menuItem?fixture.property.taxAndService.vatBpsExampleOnly:0},crypto.randomUUID())).id);
  }
  for(const item of fixture.restaurant.menu)products[item.sku]=item.stockSku?products[item.stockSku]:String((await saveMaster(admin,"products",null,{sku:`RLH-${item.sku}`,name:item.name,unit:"each",price:String(item.priceMinor/100),taxBps:fixture.property.taxAndService.vatBpsExampleOnly},crypto.randomUUID())).id);
  // Opening inventory is fixture-only evidence in a disposable database; it is not a production import.
  for(const stock of fixture.restaurant.openingStock)await input.db.prepare("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,?,?,1)").bind(warehouseId,products[stock.sku],stock.quantityMilli,stock.valueMinor).run();
  const openingEquityId=`${f.company}-hospitality-opening-equity`;
  await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'HOTEL-OPEN','Demo opening inventory equity','equity','THB',?,?)").bind(openingEquityId,f.chart,f.company,now,now).run();
  const openingValue=fixture.restaurant.openingStock.reduce((sum,item)=>sum+item.valueMinor,0);
  const {createJournal,createJournalSchema,postJournal}=await import("../../src/server/finance");
  const {resolveFlow,assignFlowPrimary}=await import("../../src/server/flow-routing");
  const opening=await createJournal(admin,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:date,currency:"THB",memo:"Fictional Riverside kitchen opening inventory",sourceType:"manual",isOpening:true,lines:[{accountId:pmsAccounts.inventory,debit:String(openingValue/100),credit:"0"},{accountId:openingEquityId,debit:"0",credit:String(openingValue/100)}]}),crypto.randomUUID());
  const flow=await resolveFlow(db,admin,"journal",String(opening.id));
  await assignFlowPrimary(db,admin,"journal",String(opening.id),{stepKey:"draft",userId:finance.userId,expectedVersion:flow.version,recordVersion:1,reason:"Independent review of fictional opening kitchen inventory",saveDefault:false},crypto.randomUUID());
  await postJournal(finance,String(opening.id),crypto.randomUUID(),{expectedVersion:1});
  const pos=async(operation:string,details:unknown)=>posCommand(db,admin,operation,details,crypto.randomUUID());
  await pos("settings.save",{enabled:true,currency:"THB",timezone:"Asia/Bangkok",cashierDiscountBps:0,accounts:{chartId:f.chart,...posAccounts},expectedVersion:0});
  const registerId=String((await pos("register.save",{warehouseId,name:fixture.restaurant.register,mode:"restaurant"})).id);
  for(const table of fixture.restaurant.tables)await pos("table.create",{registerId,name:table,seats:4});
  for(const item of fixture.restaurant.menu)await pos("catalog.save",{productId:products[item.sku],barcode:"",category:"Restaurant",kind:item.recipe?"recipe":"stock",station:item.sku==="TEA"||item.sku==="COFFEE"?"Bar":"Kitchen",recipe:item.recipe?.map(part=>({productId:products[part.sku],quantityMilli:part.quantityMilli}))??[],modifiers:[],taxInclusive:false,expectedVersion:0});
  const guide={version:1,kind:"isolated-real-workflow",title:fixture.name,origin:input.origin,companyId:f.company,propertyId,registerId,reset:{destroy:`bun run demo:destroy -- ${input.run}`,prepare:"bun run demo:prepare"},sandbox:true,actors:users,
    scenarios:[{id:"restaurant-room-charge",startingState:`Room 101 is inspected on ${date}; room ${occupied?.number??"202"} has an existing checked-in example guest. No restaurant bill exists.`,steps:[
      {actor:"frontDesk",href:`${input.origin}/holdings/riverside-lantern/pms/reservations?property=${propertyId}`,action:"Create a reservation, assign room 101, and check in the guest.",expected:"The stay appears as in-house with an open folio and inspected room."},
      {actor:"cashier",href:`${input.origin}/holdings/riverside-lantern/pos`,action:"Open a restaurant bill and add Basil chicken rice and Thai iced tea.",expected:"The bill remains open with a reviewable THB total."},
      {actor:"kitchen",href:`${input.origin}/holdings/riverside-lantern/pos?tab=kitchen`,action:"Prepare and deliver the kitchen items.",expected:"Each recipe item reaches Delivered and stock is consumed once."},
      {actor:"cashier",href:`${input.origin}/holdings/riverside-lantern/pos`,action:"Charge the complete bill to the in-house guest folio.",expected:"The bill is completed with one linked PMS charge and no POS payment."},
      {actor:"financeApprover",href:`${input.origin}/holdings/riverside-lantern/finance`,action:"Review the PMS posting and reconcile the linked source.",expected:"PMS owns the revenue posting; POS does not duplicate it."},
      {actor:"nightAuditor",href:`${input.origin}/holdings/riverside-lantern/pms/night-audit?property=${propertyId}`,action:"Review day-close blockers and close only after reconciliation.",expected:"An unresolved blocker prevents close; a resolved day has one recoverable audit result."}
    ],expected:"Exactly one POS sale, PMS folio charge and PMS revenue posting remain linked after reload."},
    {id:"pending-bank-transfer",startingState:"The cashier creates a new unpaid restaurant bill in the reset run.",steps:[
      {actor:"cashier",href:`${input.origin}/holdings/riverside-lantern/pos`,action:"Record a bank-transfer reference without imported bank evidence.",expected:"The payment stays pending and the bill cannot complete."},
      {actor:"financePreparer",href:`${input.origin}/holdings/riverside-lantern/finance/bank`,action:"Import matching sandbox bank evidence.",expected:"The bank row is available for independent verification; no money moves outside the demo."},
      {actor:"financeApprover",href:`${input.origin}/holdings/riverside-lantern/pos`,action:"Verify the exact amount, reference, currency, company and bank account.",expected:"One bank match settles one POS payment; an incorrect row remains unmatched."}
    ],expected:"Pending and verified amounts are shown separately."},
    {id:"hotel-verified-transfer",startingState:"The checked-in example guest has an open hotel folio. No hotel bank transfer has been recorded.",steps:[
      {actor:"hotelCashier",href:`${input.origin}/holdings/riverside-lantern/pms/cashier?property=${propertyId}`,action:"Open a hotel cashier shift, then record a bank-transfer reference against the guest folio.",expected:"The transfer is pending; the folio and bank ledger remain unchanged."},
      {actor:"financePreparer",href:`${input.origin}/holdings/riverside-lantern/finance/bank`,action:"Import a fictional THB statement row for the hotel's configured bank account, date, exact amount and reference.",expected:"The row is available to an independent Finance approver without settling the folio."},
      {actor:"financeApprover",href:`${input.origin}/holdings/riverside-lantern/pms/folios?property=${propertyId}`,action:"Select the exact imported row and verify the pending hotel transfer.",expected:"One PMS folio receipt, Finance payment and matched bank row are linked and survive reload."}
    ],expected:"A typed reference never settles the folio by itself; cancellation leaves no payment."},
    {id:"checkout-and-day-close",startingState:"The checked-in example guest has an open folio and the hotel business day is open. Use this after any restaurant room charge you want included.",steps:[
      {actor:"frontDesk",href:`${input.origin}/holdings/riverside-lantern/pms/front-desk?property=${propertyId}`,action:"Post the room night before checkout and review the folio balance.",expected:"The room charge appears once on the same guest folio."},
      {actor:"hotelCashier",href:`${input.origin}/holdings/riverside-lantern/pms/folios?property=${propertyId}`,action:"Open a hotel cash shift and collect the remaining folio balance using cash or an independently verified transfer.",expected:"The folio balance is zero and the receipt survives reload."},
      {actor:"frontDesk",href:`${input.origin}/holdings/riverside-lantern/pms/front-desk?property=${propertyId}`,action:"Check out the guest after the balance is settled.",expected:"The stay is checked out and the departure room needs housekeeping."},
      {actor:"hotelCashier",href:`${input.origin}/holdings/riverside-lantern/pms/folios?property=${propertyId}`,action:"Issue the folio invoice, count the hotel cash shift, and record the handover.",expected:"The invoice is paid, the folio is closed, and the shift has no unexplained variance."},
      {actor:"cashier",href:`${input.origin}/holdings/riverside-lantern/pos?tab=shifts`,action:"Resolve or cancel any open restaurant bill, then count and close the restaurant cash session.",expected:"No open restaurant bill or cash session blocks the hotel day."},
      {actor:"nightAuditor",href:`${input.origin}/holdings/riverside-lantern/pms/night-audit?property=${propertyId}`,action:"Review the hotel and restaurant controls, then start the recoverable day-close job.",expected:"The worker completes the close, advances the business date, and keeps journals balanced."}
    ],expected:"The guest folio invoice is paid, restaurant cash is counted, and the next business date opens only after Night Audit completes."},
    {id:"wrong-room-recovery",startingState:"A checked-in example guest is in room 202 and a new restaurant bill is open.",steps:[
      {actor:"cashier",href:`${input.origin}/holdings/riverside-lantern/pos`,action:"Attempt to charge a nonexistent or checked-out stay.",expected:"The bill remains open with no folio entry, stock consumption or posting."},
      {actor:"cashier",href:`${input.origin}/holdings/riverside-lantern/pos`,action:"Choose the verified in-house stay and retry with a fresh version.",expected:"Exactly one charge is created; repeating the same command key returns it."}
    ],expected:"Failed attempts do not duplicate revenue or stock movements."}]};
  return {companyId:f.company,propertyId,registerId,warehouseId,openingJournalId:opening.id,openingInventoryAccountId:pmsAccounts.inventory,roomIds:rooms,roomTypeIds:types,rateIds:rates,productIds:products,users,guide};
}
