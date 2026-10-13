/** Persistent fictional practice fixtures. This command never touches staging. */
import { readFile, writeFile, mkdir, rename } from 'node:fs/promises';
import { dirname, resolve, join } from 'node:path';
import { createHash, randomBytes } from 'node:crypto';
import { hashPassword } from 'better-auth/crypto';
import { assertHospitalityDevelopmentTarget } from "../../src/lib/shared/hospitality-seed-target";
import type { WorkContext } from "../../src/server/modules/identity_access/public/authorization";
import { businessRoles } from "../../src/lib/identity_access/business-roles";
import { pmsAccountKeys, records, record, plusDays } from "../../src/server/modules/pms/public/rules/pms-model";
import { mock } from 'bun:test';
// CLI has no page renderer. Fail closed if a domain command attempts navigation.
mock.module('next/navigation',()=>({forbidden:()=>{throw new Error('CLI_PAGE_FORBIDDEN');},redirect:()=>{throw new Error('CLI_REDIRECT_FORBIDDEN');}}));
const {pmsAccountCategory,pmsAccountCode}=await import("../../src/server/modules/finance/public/hospitality-accounting");
import { PostgresDatabase } from "../../src/server/modules/runtime/adapters/runtime/postgres";
const { pmsCommand, loadPms, drainPms }=await import("../../src/server/modules/pms/public/pms");
const { posCommand, one }=await import("../../src/server/modules/pos/public/pos-core");
const { saveMaster }=await import("../../src/server/modules/commerce/public/commerce");
const { countSchema, saveCount, transitionCount }=await import("../../src/server/modules/inventory/public/inventory-counts");
const { importBankStatement }=await import("../../src/server/modules/finance/public/finance");
const {confirmManualRate,manualRateSchema}=await import("../../src/server/modules/finance/public/company-money");

const manifestPath=process.env.ERPX_DATABASE_MANIFEST;
if(!manifestPath)throw new Error('ERPX_DATABASE_MANIFEST_REQUIRED');
// Check both manifests before opening any connection, including the control DB.
const manifest=JSON.parse(await readFile(manifestPath,'utf8'));
assertHospitalityDevelopmentTarget(manifest,process.env.ERPX_ENVIRONMENT??process.env.NODE_ENV);
const jobsPath=join(dirname(manifestPath),'databases-jobs.json');
const jobsManifest=JSON.parse(await readFile(jobsPath,'utf8'));
assertHospitalityDevelopmentTarget(jobsManifest,process.env.ERPX_ENVIRONMENT??process.env.NODE_ENV);
process.env.ERPX_DATABASE_MANIFEST=jobsPath;process.env.ERPX_PROCESS_ROLE='jobs';
const {getRuntime,closeRuntime}=await import("../../src/server/modules/runtime/adapters/runtime/env");
const env=getRuntime(),db=new PostgresDatabase({connectionString:jobsManifest.databases.H001_DB,max:2}),control=env.CONTROL_DB;

const namespace='hospitality-demo-v1',directory=resolve('.scratch/dev/hospitality');
await mkdir(directory,{recursive:true,mode:0o700});
const digest=createHash('sha256').update(new URL(jobsManifest.databases.H001_DB).pathname).digest('hex').slice(0,12);
const statePath=join(directory,`state-${digest}.json`);
type Result={id:string;[key:string]:unknown};
type Step={input:unknown;result?:Result};
type State={date:string;steps:Record<string,Step>;users:{email:string;password:string;role:string;companyId:string}[]};
const state:State=await readFile(statePath,'utf8').then(JSON.parse).catch(error=>{if((error as NodeJS.ErrnoException).code!=='ENOENT')throw error;return {date:new Intl.DateTimeFormat('en-CA',{timeZone:'Asia/Bangkok',year:'numeric',month:'2-digit',day:'2-digit'}).format(new Date()),steps:{},users:[]};});
async function persist(){const tmp=`${statePath}.tmp`;await writeFile(tmp,JSON.stringify(state,null,2),{mode:0o600});await rename(tmp,statePath);}
const commandKey=(label:string)=>`${namespace}-${createHash('sha256').update(label).digest('hex').slice(0,32)}`;
async function step(label:string,input:unknown,fn:(input:unknown,key:string)=>Promise<unknown>):Promise<Result>{
  const saved=state.steps[label];if(saved?.result)return saved.result;
  if(!saved){state.steps[label]={input};await persist();}
  const result=await fn(state.steps[label].input,commandKey(label)) as Result;
  state.steps[label].result=result;await persist();return result;
}
const fixture=JSON.parse(await readFile(resolve('fixtures/hospitality/pilot-example.json'),'utf8')) as {
  restaurant:{openingStock:{sku:string;unit:string;quantityMilli:number;valueMinor:number}[];menu:{sku:string;name:string;priceMinor:number;stockSku?:string;recipe?:{sku:string;quantityMilli:number}[]}[];tables:string[]};
};
const date=state.date,holdingSlug='acme',holdingId='h001',origin='https://console-dev-erpx.jesadakorn.com';
const companies=[{id:'hospitality-setup-practice-demo',name:'Hospitality Setup Practice Demo',branches:['practice'],configured:false},{id:'riverside-lantern-hotel-kitchen-demo',name:'Riverside Lantern Hotel & Kitchen Demo',branches:['rehearsal','scenarios'],configured:true}];
const actorRoles=[['administrator','holding_owner'],['financePreparer','finance_preparer'],['financeApprover','finance_approver'],['frontDesk','pms_frontdesk'],['hotelCashier','pms_cashier'],['hotelManager','pms_manager'],['housekeeper','pms_housekeeping'],['inspector','pms_supervisor'],['cashier','pos_cashier'],['kitchen','pos_kitchen'],['restaurantManager','pos_manager'],['nightAuditor','pms_night_auditor']] as const;
const summary:Record<string,unknown>[]=[];
try {
  // A session lock serializes seed reruns without holding an operational transaction open.
  await db.withSession(async lock=>{
    if(!await lock.prepare('SELECT pg_try_advisory_lock(781904302) acquired').first<boolean>('acquired'))throw new Error('HOSPITALITY_SEED_ALREADY_RUNNING');
    try {
      for(const company of companies){
        const now=Date.now(),actors:Record<string,WorkContext>={};
        await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,?,?) ON CONFLICT DO NOTHING").bind(company.id,company.id,company.name,company.id).run();
        for(const name of company.branches){const branch=`${company.id}-${name}`;await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,?,?,?,?) ON CONFLICT DO NOTHING").bind(branch,branch,`${company.name} · ${name}`,company.id,company.id,branch).run();}
        const level=`${company.id}-level`;
        await db.prepare("INSERT INTO staff_levels(id,company_id,code,name,rank,created_at,updated_at) VALUES(?,?,'DEMO','Demo staff',1,?,?) ON CONFLICT DO NOTHING").bind(level,company.id,now,now).run();
        for(const [label,role] of actorRoles){
          const userId=`${company.id}-${label}`,assignmentId=`${userId}-access`,email=`${label.toLowerCase()}@${company.id}.example`;
          let credential=state.users.find(u=>u.email===email);
          if(!credential){credential={email,password:`Practice-${randomBytes(18).toString('base64url')}!`,role,companyId:company.id};state.users.push(credential);await persist();}
          const passwordHash=await hashPassword(credential.password);
          await control.batch([
            control.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?) ON CONFLICT DO NOTHING').bind(userId,`${company.name} ${label}`,email,now,now),
            control.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?) ON CONFLICT DO NOTHING").bind(`${userId}-account`,userId,userId,passwordHash,now,now),
            control.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?) ON CONFLICT DO NOTHING").bind(`${userId}-principal`,userId,label,now),
          ]);
          const modules=role==='holding_owner'?['pms','pos','finance','inventory','commerce','organization','iam','people']:businessRoles[role];
          await db.batch([
            db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,?,?,?,0,9999999999999,'holding') ON CONFLICT DO NOTHING").bind(assignmentId,userId,role,JSON.stringify(modules),company.id),
            db.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,email,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'TH',?,?,?) ON CONFLICT DO NOTHING").bind(`${userId}-person`,userId,company.id,label,email,userId,now,now),
            db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at,level_id) VALUES(?,?,?,?,'employee','salaried','active','2026-01-01',?,'THB',?,?,?,?) ON CONFLICT DO NOTHING").bind(`${userId}-engagement`,`${userId}-person`,company.id,`${company.id}-${company.branches[0]}`,role,userId,now,now,level),
          ]);
          actors[label]={userId,assignmentId,role,companyId:company.id,branchId:null,departmentId:null,holdingId,holdingSlug,holdingName:'ACME',name:label,email,assignmentScope:'holding',inherited:false,modules:[...modules],assignments:[],workspaces:[],scope:{kind:'holding',holdingId,holdingSlug,holdingName:'ACME',companyId:company.id}};
        }
        const admin=actors.administrator,finance=actors.financeApprover;
        await step(`${company.id}:default-currency`,{},async()=>{await db.prepare("UPDATE company_money_settings SET default_currency='THB',updated_by=?,updated_at=? WHERE company_id=?").bind(admin.userId,Date.now(),company.id).run();return {id:company.id};});
        await step(`${company.id}:fictional-fx`,{currency:'THB',effectiveDate:plusDays(date,-1),currencyPerUsd:'36',note:'Fictional rehearsal rate; not market data'},(input,key)=>confirmManualRate(finance,manualRateSchema.parse(input),key));
        if(!company.configured){summary.push({companyId:company.id,setup:`${origin}/holdings/${holdingSlug}/hospitality/setup?branch=${company.id}-practice`,state:'Empty POS/PMS configuration; company, branch and staff ready'});continue;}
        const chart=`${company.id}-chart`,period=`${company.id}-period`;
        await db.batch([
          db.prepare("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES(?,?,'Fictional hospitality THB','THB',?,?,?) ON CONFLICT DO NOTHING").bind(chart,company.id,admin.userId,now,now),
          db.prepare("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES(?,?,'Fictional practice period',?,?,?) ON CONFLICT DO NOTHING").bind(period,company.id,`${date.slice(0,4)}-01-01`,`${Number(date.slice(0,4))+1}-12-31`,now),
        ]);
        const accountIds=Object.fromEntries(pmsAccountKeys.map(k=>[k,`${company.id}-${k}`]));
        const extra=[{key:'payable',code:'DEMO-PAYABLE',category:'liability'},{key:'variance',code:'5400',category:'expense'}];
        for(const spec of [...pmsAccountKeys.map(k=>({key:k,code:pmsAccountCode(k),category:pmsAccountCategory(k)})),...extra])await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',?,?,?,?) ON CONFLICT DO NOTHING").bind(`${company.id}-${spec.key}`,chart,company.id,spec.code,`Fictional ${spec.key}`,spec.category,spec.key==='bank'?1:0,spec.key==='bank'?'bank':'none',now,now).run();
        await db.prepare("INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,output_tax_account_id,input_tax_account_id,cash_account_id) VALUES(?,?,'THB',?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
          .bind(company.id,chart,accountIds.receivable,`${company.id}-payable`,accountIds.revenue,accountIds.inventory,`${company.id}-payable`,accountIds.consumption,accountIds.tax,accountIds.prepaidTax,accountIds.cash).run();
        const products:Record<string,string>={};
        for(const stock of fixture.restaurant.openingStock){const menuItem=fixture.restaurant.menu.find(m=>m.stockSku===stock.sku);products[stock.sku]=(await step(`${company.id}:product:${stock.sku}`,{sku:`DEMO-${stock.sku}`,name:menuItem?.name??`Fictional ${stock.sku}`,unit:stock.unit,price:String((menuItem?.priceMinor??100)/100),taxBps:menuItem?700:0},(input,key)=>saveMaster(admin,'products',null,input,key))).id;}
        for(const menu of fixture.restaurant.menu)if(!menu.stockSku)products[menu.sku]=(await step(`${company.id}:menu-product:${menu.sku}`,{sku:`DEMO-${menu.sku}`,name:menu.name,unit:'each',price:String(menu.priceMinor/100),taxBps:700},(input,key)=>saveMaster(admin,'products',null,input,key))).id;
        products.LINEN=(await step(`${company.id}:linen-product`,{sku:'DEMO-LINEN',name:'Fictional bath linen',unit:'piece',price:'0',taxBps:0},(input,key)=>saveMaster(admin,'products',null,input,key))).id;
        for(const name of company.branches){
          const prefix=`${company.id}:${name}`,branch=`${company.id}-${name}`,clean=name==='rehearsal';
          const warehouse=(await step(`${prefix}:warehouse`,{code:`DEMO-${name.toUpperCase()}`,name:`Kitchen · ${name}`,branchId:branch},(input,key)=>saveMaster(admin,'warehouses',null,input,key))).id;
          const property=(await step(`${prefix}:property`,{operation:'property.create',input:{name:`Riverside Lantern · ${name}`,branchId:branch,businessDate:clean?date:plusDays(date,-1)}},(input,key)=>pmsCommand(db,admin,input,key))).id;
          const pms=async(label:string,op:string,input:unknown={},actor=admin)=>{
            const snapshot=await loadPms(db,admin,property);
            return step(`${prefix}:${label}`,{operation:op,propertyId:property,expectedVersion:snapshot.property.version,input},(body,key)=>pmsCommand(db,actor,body,key));
          };
          await pms('hotel-policy','property.save',{name:`Riverside Lantern · ${name}`,country:'TH',timezone:'Asia/Bangkok',checkIn:'14:00',checkOut:'12:00',cancellationPolicy:'Fictional practice policy: free cancellation until the day before arrival.'});
          await pms('accounting','accounting.bootstrap',{chartId:chart,accounts:accountIds,taxBps:700,serviceBps:0,taxInclusive:false,advanceTax:true,financeReviewed:true});
          await pms('approve','accounting.approve',{},finance);
          const types:Record<string,string>={},rates:Record<string,string>={},rooms:Record<string,string>={};
          for(const [type,price] of [['Standard',180000],['Deluxe',260000]] as const){types[type]=(await pms(`type:${type}`,'room_type.save',{name:type,capacity:2,amenities:'Fictional demo room'})).id;rates[type]=(await pms(`rate:${type}`,'rate.save',{name:`${type} flexible`,typeId:types[type],from:plusDays(date,-1),to:plusDays(date,365),price})).id;}
          for(const number of ['101','102','103','104','201','202','203','204'])rooms[number]=(await pms(`room:${number}`,'room.save',{number,typeId:types[number[0]==='1'?'Standard':'Deluxe'],floor:number[0],active:true})).id;
          await pms('enable','property.enable');
          for(const [number,id] of Object.entries(rooms)){
            const task=(await pms(`inspect-task:${number}`,'task.create',{roomId:id,type:'arrival',date:clean?date:plusDays(date,-1)})).id;
            const checklist=record(await loadPms(db,admin,property),'task',task).data.checklist;
            await pms(`clean:${number}`,'task.complete',{id:task,checked:checklist},actors.housekeeper);
            await pms(`inspect:${number}`,'task.inspect',{id:task},actors.inspector);
          }
          const count=(await step(`${prefix}:opening-count`,countSchema.parse({warehouseId:warehouse,countDate:clean?date:plusDays(date,-1),reason:'Fictional independently counted opening ingredient stock',lines:fixture.restaurant.openingStock.map(s=>({productId:products[s.sku],countedQuantity:String(s.quantityMilli/1000),unitCost:(s.valueMinor/s.quantityMilli*10).toFixed(2)}))}),(input,key)=>saveCount(admin,null,countSchema.parse(input),key))).id;
          for(const [action,version] of [['submit',1],['approve',2],['post',3]] as const)await step(`${prefix}:count-${action}`,{expectedVersion:version,attachmentSetIds:[],reason:'Independent fictional opening stock review'},(input,key)=>transitionCount(action==='submit'?admin:finance,count,action,input as {expectedVersion:number;attachmentSetIds:string[];reason:string},key));
          const pos=async(label:string,op:string,input:unknown,actor=admin)=>step(`${prefix}:${label}`,input,(body,key)=>posCommand(db,actor,op,body,key));
          let register:string;
          if(clean)register=String((await pos('bootstrap-pos','settings.bootstrap',{mode:'restaurant',registerName:'Rehearsal kitchen',warehouseId:warehouse})).registerId);
          else register=(await pos('register','register.save',{name:'Scenario kitchen',warehouseId:warehouse,mode:'restaurant'})).id;
          for(const table of fixture.restaurant.tables)await pos(`table:${table}`,'table.create',{registerId:register,name:table,seats:4});
          for(const menu of fixture.restaurant.menu)await pos(`catalog:${menu.sku}`,'catalog.save',{productId:products[menu.sku],barcode:'',category:'Restaurant',kind:menu.recipe?'recipe':'stock',station:['TEA','COFFEE'].includes(menu.sku)?'Bar':'Kitchen',recipe:menu.recipe?.map(p=>({productId:products[p.sku],quantityMilli:p.quantityMilli}))??[],modifiers:[],taxInclusive:false,expectedVersion:clean?0:1});
          summary.push({companyId:company.id,branchId:branch,propertyId:property,registerId:register,setup:`${origin}/holdings/${holdingSlug}/hospitality/setup?branch=${branch}&property=${property}`,hotel:`${origin}/holdings/${holdingSlug}/pms/settings?property=${property}`,restaurant:`${origin}/holdings/${holdingSlug}/pos?tab=settings`,stockCount:count,cleanRehearsal:clean});
          if(clean)continue;
          // Complete a seed-owned quiet day; no other company's jobs are drained.
          const night=(await pms('completed-audit','night.start',{},actors.nightAuditor)).id;
          await drainPms(db,property,night);
          if(record(await loadPms(db,admin,property),'night',night).data.state!=='completed')throw new Error('SEED_NIGHT_AUDIT_INCOMPLETE');
          const reserve=async(label:string,type='Standard',arrival=date)=>{
            const guest=(await pms(`guest:${label}`,'guest.save',{name:`Fictional ${label} Guest`,email:`${label}@guest.example`},actors.frontDesk)).id;
            const booking=(await pms(`booking:${label}`,'reservation.create',{guestId:guest,source:'staff',rooms:[{typeId:types[type],rateId:rates[type],arrival,departure:plusDays(arrival,2),adults:2,quantity:1}]},actors.frontDesk)).id;
            const stay=records(await loadPms(db,admin,property),'stay').find(s=>s.data.reservationId===booking);if(!stay)throw new Error('SEED_STAY_MISSING');return {guest,booking,stay:stay.id,guestAccount:stay.data.guestAccountId};
          };
          const inhouse=await reserve('Inhouse','Deluxe');await pms('assign-inhouse','stay.assign',{id:inhouse.stay,roomId:rooms['202']},actors.frontDesk);await pms('checkin-inhouse','stay.check_in',{id:inhouse.stay},actors.frontDesk);
          const arrival=await reserve('Arrival');await pms('assign-arrival','stay.assign',{id:arrival.stay,roomId:rooms['101']},actors.frontDesk);
          const cancellation=await reserve('Cancellation');await pms('cancel-reservation','reservation.cancel',{id:cancellation.booking,reason:'Fictional cancellation example'},actors.frontDesk);
          const noshow=await reserve('NoShow');await pms('noshow-reservation','reservation.no_show',{id:noshow.booking,reason:'Fictional no-show example'},actors.frontDesk);
          const departing=await reserve('Checkout');await pms('assign-checkout','stay.assign',{id:departing.stay,roomId:rooms['102']},actors.frontDesk);await pms('checkin-checkout','stay.check_in',{id:departing.stay},actors.frontDesk);
          const shift=(await pms('hotel-shift','shift.open',{name:'Fictional hotel till',opening:50000},actors.hotelCashier)).id;
          await pms('advance','payment.receive',{guestAccountId:inhouse.guestAccount,shiftId:shift,amount:100000,method:'cash'},actors.hotelCashier);
          await pms('deposit','payment.deposit',{guestAccountId:inhouse.guestAccount,shiftId:shift,amount:50000,method:'cash'},actors.hotelCashier);
          const refund=(await pms('refund-request','refund.request',{guestAccountId:inhouse.guestAccount,amount:10000,pocket:'deposit',reason:'Fictional deposit refund'},actors.hotelCashier)).id;
          await pms('refund-approve','refund.approve',{id:refund},finance);await pms('refund-pay','refund.pay',{id:refund,shiftId:shift,method:'cash'},actors.hotelCashier);
          const transfer=(await pms('pending-transfer','payment.request_transfer',{guestAccountId:arrival.guestAccount,shiftId:shift,amount:30000,reference:'FICTIONAL-HOTEL-PENDING'},actors.hotelCashier)).id;
          const verified=(await pms('verify-transfer-record','payment.request_transfer',{guestAccountId:inhouse.guestAccount,shiftId:shift,amount:20000,reference:'FICTIONAL-HOTEL-VERIFIED'},actors.hotelCashier)).id;
          await step(`${prefix}:bank-evidence`,{bankAccount:accountIds.bank,statementNumber:'FICTIONAL-HOTEL-1',statementDate:date,opening:'0',closing:'200',sourceHash:commandKey(`${prefix}:bank`),rows:[{externalId:'FICTIONAL-HOTEL-VERIFIED',transactionDate:date,description:'Fictional evidence only',amount:'200',currency:'THB'}]},(input,key)=>importBankStatement(admin,input as Parameters<typeof importBankStatement>[1],key));
          const bank=await one(db,"SELECT id,version FROM finance_bank_transactions WHERE company_id=? AND external_id='FICTIONAL-HOTEL-VERIFIED'",company.id);
          await pms('verify-transfer','payment.verify_transfer',{id:verified,expectedTransferVersion:1,bankTransactionId:bank!.id,bankVersion:Number(bank!.version)},finance);
          const charge=(await pms('correction-charge','charge.add',{guestAccountId:inhouse.guestAccount,price:10000,description:'Fictional incorrect minibar charge'},actors.hotelCashier)).id;
          const correction=(await pms('correction-request','charge.correction.request',{id:charge,reason:'Fictional wrong-room correction'},actors.hotelCashier)).id;
          await pms('correction-review','charge.correction.review',{id:correction,expectedVersion:1,action:'approve',reason:'Independent fictional correction review'},finance);
          await pms('checkout-advance','payment.receive',{guestAccountId:departing.guestAccount,shiftId:shift,amount:192600,method:'cash'},actors.hotelCashier);
          await pms('checkout','stay.check_out',{id:departing.stay},actors.frontDesk);await pms('close-checkout-guestAccount','guest_account.close',{id:departing.guestAccount},actors.hotelCashier);
          const minibar=(await pms('minibar-item','item.save',{name:'Fictional minibar water',kind:'minibar',productId:products.WATER,warehouseId:warehouse,price:4000})).id;
          await pms('minibar-charge','minibar.charge',{itemId:minibar,quantity:1,roomId:rooms['202'],guestAccountId:inhouse.guestAccount,reason:'Fictional water consumed'},actors.hotelCashier);
          const linenCount=(await step(`${prefix}:linen-count`,countSchema.parse({warehouseId:warehouse,countDate:date,reason:'Fictional independently counted bath linen',lines:[{productId:products.LINEN,countedQuantity:'100',unitCost:'30.00'}]}),(input,key)=>saveCount(admin,null,countSchema.parse(input),key))).id;
          for(const [action,version] of [['submit',1],['approve',2],['post',3]] as const)await step(`${prefix}:linen-count-${action}`,{expectedVersion:version,attachmentSetIds:[],reason:'Independent fictional linen stock review'},(input,key)=>transitionCount(action==='submit'?admin:finance,linenCount,action,input as {expectedVersion:number;attachmentSetIds:string[];reason:string},key));
          // Preserve earlier practice records while making the linen stock example distinct from drinks.
          const oldLinen=state.steps[`${prefix}:linen-item`]?.result?.id;
          if(oldLinen){const old=record(await loadPms(db,admin,property),'item',oldLinen);if(old.data.name==='Fictional linen'&&old.data.productId===products.WATER)await pms('legacy-linen-label','item.save',{id:old.id,name:'Legacy fictional stock stand-in',kind:'linen',productId:products.WATER,warehouseId:warehouse,price:old.data.price});}
          const linen=(await pms('linen-item-v2','item.save',{name:'Fictional bath linen',kind:'linen',productId:products.LINEN,warehouseId:warehouse,price:0})).id;
          await pms('linen-receive-v2','linen.receive',{itemId:linen,quantity:10},actors.housekeeper);await pms('linen-issue-v2','linen.issue',{itemId:linen,quantity:2,roomId:rooms['202']},actors.housekeeper);
          await pms('maintenance','maintenance.open',{roomId:rooms['104'],from:date,to:plusDays(date,3),reason:'Fictional maintenance example'},actors.hotelManager);
          const posShift=(await pos('pos-shift','shift.open',{registerId:register,openingMinor:50000},actors.cashier)).id;
          const sale=async(label:string,sku:string)=>{
            const id=(await pos(`sale:${label}`,'sale.create',{shiftId:posShift,serviceMode:'dine_in'},actors.cashier)).id;
            const s=await one(db,'SELECT version FROM pos_sales WHERE id=?',id);
            await pos(`line:${label}`,'sale.add',{id,expectedVersion:Number(s!.version),productId:products[sku],quantityMilli:1000,modifierIds:[]},actors.cashier);
            return id;
          };
          const sv=async(id:string)=>Number((await one(db,'SELECT version FROM pos_sales WHERE id=?',id))!.version);
          const prepare=async(label:string,id:string,until='deliver')=>{
            const line=await one(db,'SELECT fulfillment_id FROM pos_lines WHERE sale_id=?',id);
            for(const op of ['send','start','ready','deliver']){const f=await one(db,'SELECT version FROM pos_fulfillments WHERE id=?',line!.fulfillment_id);await pos(`${label}:${op}`,`kitchen.${op}`,{id:line!.fulfillment_id,expectedVersion:Number(f!.version)},actors.kitchen);if(op===until)break;}
          };
          const roomBill=await sale('RoomCharge','RICE-CHICKEN');await prepare('room-order',roomBill);
          await pos('room-charge','sale.charge_to_room',{id:roomBill,expectedVersion:await sv(roomBill),propertyId:property,stayId:inhouse.stay,expectedPropertyVersion:(await loadPms(db,admin,property)).property.version},actors.cashier);
          const completed=await sale('Completed','WATER');
          await pos('cash-payment','payment.create',{id:completed,method:'cash',amountMinor:4280,tenderedMinor:4280,reference:'FICTIONAL-CASH',expectedVersion:await sv(completed)},actors.cashier);
          await pos('complete-cash','sale.complete',{id:completed,expectedVersion:await sv(completed)},actors.cashier);
          const returnLine=await one(db,'SELECT id FROM pos_lines WHERE sale_id=?',completed);
          const posRefund=(await pos('pos-refund-request','refund.request',{id:completed,expectedVersion:await sv(completed),reason:'Fictional bottled-water return',lines:[{lineId:returnLine!.id,quantityMilli:1000,restock:true}],payoutShiftId:posShift},actors.cashier)).id;
          await pos('pos-refund-approve','refund.approve',{id:posRefund,expectedVersion:1,reason:'Fictional cash returned; independent manager verified'},actors.restaurantManager);
          const pendingPos=await sale('PendingTransfer','WATER');
          await pos('pending-pos-payment','payment.create',{id:pendingPos,expectedVersion:await sv(pendingPos),method:'transfer',amountMinor:4280,tenderedMinor:4280,reference:'FICTIONAL-POS-PENDING'},actors.cashier);
          const verifiedPos=await sale('VerifiedTransfer','WATER');
          const posPayment=(await pos('verified-pos-payment','payment.create',{id:verifiedPos,expectedVersion:await sv(verifiedPos),method:'transfer',amountMinor:4280,tenderedMinor:4280,reference:'FICTIONAL-POS-VERIFIED'},actors.cashier)).id;
          await step(`${prefix}:pos-bank-evidence`,{bankAccount:accountIds.bank,statementNumber:'FICTIONAL-POS-1',statementDate:date,opening:'0',closing:'42.80',sourceHash:commandKey(`${prefix}:pos-bank`),rows:[{externalId:'FICTIONAL-POS-VERIFIED',transactionDate:date,description:'Fictional restaurant bank evidence only',amount:'42.80',currency:'THB'}]},(input,key)=>importBankStatement(admin,input as Parameters<typeof importBankStatement>[1],key));
          const posBank=await one(db,"SELECT id,version FROM finance_bank_transactions WHERE company_id=? AND external_id='FICTIONAL-POS-VERIFIED'",company.id);
          await pos('verify-pos-transfer','payment.verify_transfer',{id:posPayment,expectedVersion:2,bankTransactionId:posBank!.id,bankVersion:Number(posBank!.version)},finance);
          await pos('complete-transfer-sale','sale.complete',{id:verifiedPos,expectedVersion:await sv(verifiedPos)},actors.cashier);
          const queued=await sale('Queued','TEA');await prepare('queued-order',queued,'send');
          const held=await sale('Held','TEA');await pos('hold-sale','sale.hold',{id:held,expectedVersion:await sv(held)},actors.cashier);
          const preparing=await sale('Preparing','COFFEE');await prepare('preparing-order',preparing,'start');
          const ready=await sale('Ready','RICE-VEG');await prepare('ready-order',ready,'ready');
          const waste=await sale('Waste','NOODLES');await prepare('waste-order',waste,'start');
          const wf=await one(db,'SELECT f.id,f.version FROM pos_fulfillments f JOIN pos_lines l ON l.fulfillment_id=f.id WHERE l.sale_id=?',waste);
          await pos('waste-cancel','kitchen.cancel',{id:wf!.id,expectedVersion:Number(wf!.version),reason:'Fictional kitchen waste'},actors.restaurantManager);
          await pos('cancel-waste-sale','sale.cancel',{id:waste,expectedVersion:await sv(waste),reason:'Fictional cancelled order'},actors.cashier);
          const hotelCash=record(await loadPms(db,admin,property),'shift',shift).data.cash;
          await pms('hotel-variance','shift.close',{id:shift,counted:hotelCash-100,handover:'Fictional THB 1 variance pending independent review'},actors.hotelCashier);
          const varianceRegister=(await pos('variance-register','register.save',{name:'Fictional variance till',warehouseId:warehouse,mode:'restaurant'})).id;
          const varianceShift=(await pos('variance-shift','shift.open',{registerId:varianceRegister,openingMinor:10000},actors.restaurantManager)).id;
          const varianceVersion=await one(db,'SELECT version FROM pos_shifts WHERE id=?',varianceShift);
          await pos('pos-variance-close','shift.close',{id:varianceShift,expectedVersion:Number(varianceVersion!.version),countedMinor:9900,reason:'Fictional THB 1 drawer variance'},actors.restaurantManager);
          const blocked=await step(`${prefix}:blocked-audit-attempt`,{},async()=>{
            const snapshot=await loadPms(db,admin,property);
            try{await pmsCommand(db,actors.nightAuditor,{operation:'night.start',propertyId:property,expectedVersion:snapshot.property.version,input:{}},commandKey(`${prefix}:blocked-night`));}
            catch(error){const code=(error as {code?:string}).code;if(!['PMS_DAY_RECONCILIATION_BLOCKED','PMS_NIGHT_BLOCKED'].includes(code??''))throw error;return {id:property,code,reason:(error as Error).message};}
            throw new Error('SEED_EXPECTED_DAY_CLOSE_BLOCKER');
          });
          summary.push({scenarioRecords:{inhouse,arrival,departing,cancellation,noshow,pendingTransfer:transfer,verifiedTransfer:verified,refund,correction,linen,linenCount,completedNight:night,roomBill,completed,posRefund,pendingPos,verifiedPos,queued,held,preparing,ready,waste,varianceShift,blockedAudit:blocked},dayClose:'Blocked by arrival/no-show decisions, open POS bills/cash session and unexplained hotel variance'});
        }
      }
    }finally{await lock.prepare('SELECT pg_advisory_unlock(781904302)').first();}
  });
  await writeFile(join(directory,'credentials.json'),JSON.stringify({origin,users:state.users},null,2),{mode:0o600});
  await writeFile(join(directory,'scenario-guide.json'),JSON.stringify({fictional:true,date,holdingSlug,companies:summary,rehearsal:['Select the demo company using account switcher','Reserve → assign inspected room → check in','Open counted hotel and restaurant cash sessions','Order → kitchen deliver → charge to room','Settle guest account → check out → close guest account','Count cash → close sessions → review Night Audit'],paymentProviders:'Disabled; use fictional cash and imported local evidence only'},null,2),{mode:0o600});
  const lines=['# Fictional hospitality rehearsal', '', `Development console: ${origin}`, '', 'Credentials are in credentials.json in this private directory. Choose the matching company using the account switcher. These examples use fictional cash and local bank evidence; payment providers and external messaging are disabled.', '', '## First-time setup', '', `Open [Hotel + Restaurant Setup](${origin}/holdings/${holdingSlug}/hospitality/setup) as the practice company administrator. Company, branch and staff access exist; POS/PMS configuration is intentionally empty.`, '', '## Clean rehearsal', '', 'Choose Riverside Lantern Hotel & Kitchen Demo and the rehearsal branch. Eight inspected rooms, room prices, approved accounts, six tables, recipes and opening stock are ready. Open cash sessions with counted floats, then reserve → check in → order → kitchen deliver → charge to room → settle guest account → check out → close the day.', '', '## Populated scenarios', ''];
  for(const entry of summary){
    if(entry.setup)lines.push(`- [${entry.companyId==='hospitality-setup-practice-demo'?'Empty practice setup':entry.cleanRehearsal?'Clean rehearsal setup':'Scenario setup'}](${entry.setup})`);
    if(entry.hotel)lines.push(`- [Hotel settings](${entry.hotel})`);
    if(entry.restaurant)lines.push(`- [Restaurant settings](${entry.restaurant})`);
    const r=entry.scenarioRecords as Record<string,unknown>|undefined;
    if(r){const property=String((r.blockedAudit as Result).id);
      for(const label of ['inhouse','arrival','departing','cancellation','noshow']){const stay=r[label] as {guestAccount:string};lines.push(`- [${label} guestAccount](${origin}/holdings/${holdingSlug}/pms/guest-accounts?property=${property}&guestAccount=${stay.guestAccount})`);}
      for(const label of ['roomBill','completed','pendingPos','verifiedPos','queued','held','preparing','ready','waste'])lines.push(`- [${label} bill](${origin}/holdings/${holdingSlug}/pos?sale=${r[label]})`);
      lines.push(`- [Independent linen stock count](${origin}/holdings/${holdingSlug}/stock-counts/${r.linenCount})`);
      lines.push(`- [Completed Night Audit](${origin}/holdings/${holdingSlug}/pms/night-audit?property=${property}&night=${r.completedNight})`, `- Blocked day-close attempt: ${(r.blockedAudit as Result).code}. Current arrivals, open bills, a cash session and an unexplained variance intentionally prevent closing.`);
    }
  }
  await writeFile(join(directory,'scenario-guide.md'),lines.join('\n')+'\n',{mode:0o600});
  console.log(JSON.stringify({seed:'hospitality',companies:2,guide:join(directory,'scenario-guide.md'),credentials:join(directory,'credentials.json'),completedSteps:Object.keys(state.steps).length}));
}finally{await db.close();await closeRuntime();}
