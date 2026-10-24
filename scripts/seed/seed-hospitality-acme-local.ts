/** Add fictional hospitality rehearsal data to existing development Acme/Bangkok only. */
import { readFile, writeFile, mkdir, rename } from 'node:fs/promises';
import { dirname, resolve, join } from 'node:path';
import { createHash, randomBytes } from 'node:crypto';
import { hashPassword } from 'better-auth/crypto';
import { assertHospitalityDevelopmentTarget } from "@erpx/feature-shared/lib/hospitality-seed-target";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { businessRoles } from "@erpx/feature-identity_access/lib/business-roles";
import { records, record, plusDays } from "@erpx/server-pms/public/rules/pms-model";
import { mock } from 'bun:test';
// CLI has no page renderer. Fail closed if a domain command attempts navigation.
mock.module('next/navigation',()=>({forbidden:()=>{throw new Error('CLI_PAGE_FORBIDDEN');},redirect:()=>{throw new Error('CLI_REDIRECT_FORBIDDEN');}}));
import { PostgresDatabase } from "@erpx/server-runtime/adapters/runtime/postgres";
const { pmsCommand, loadPms, drainPms }=await import("../../packages/server-pms/src/public/pms");
const {posCommand}=await import("@erpx/server-pos/public/pos-core");
const {one}=await import("@erpx/server-runtime/public/runtime/sql");
const { saveMaster }=await import("@erpx/server-commerce/public/commerce");
const { countSchema, saveCount, transitionCount }=await import("@erpx/server-inventory/public/inventory-counts");
const {resolveFlow,assignFlowPrimary}=await import("@erpx/server-workflow/public/flow-routing");

const manifestPath=process.env.ERPX_DATABASE_MANIFEST;
if(!manifestPath)throw new Error('ERPX_DATABASE_MANIFEST_REQUIRED');
// Check both manifests before opening any connection, including the control DB.
const manifest=JSON.parse(await readFile(manifestPath,'utf8'));
assertHospitalityDevelopmentTarget(manifest,process.env.ERPX_ENVIRONMENT??process.env.NODE_ENV);
const jobsPath=join(dirname(manifestPath),'databases-jobs.json');
const jobsManifest=JSON.parse(await readFile(jobsPath,'utf8'));
assertHospitalityDevelopmentTarget(jobsManifest,process.env.ERPX_ENVIRONMENT??process.env.NODE_ENV);
process.env.ERPX_DATABASE_MANIFEST=jobsPath;process.env.ERPX_PROCESS_ROLE='jobs';
const {getRuntime,closeRuntime}=await import("@erpx/server-runtime/adapters/runtime/env");
const env=getRuntime(),db=new PostgresDatabase({connectionString:jobsManifest.databases.H001_DB,max:2}),control=env.CONTROL_DB;

const namespace='acme-hospitality-v1',directory=resolve('.scratch/dev/hospitality-acme');
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
const actorRoles=[['administrator','holding_owner'],['financePreparer','finance_preparer'],['financeApprover','finance_approver'],['frontDesk','pms_frontdesk'],['hotelCashier','pms_cashier'],['hotelManager','pms_manager'],['housekeeper','pms_housekeeping'],['inspector','pms_supervisor'],['cashier','pos_cashier'],['kitchen','pos_kitchen'],['restaurantManager','pos_manager'],['nightAuditor','pms_night_auditor']] as const;
const company={id:'acme-co',name:'Acme hospitality practice'};
try { await db.withSession(async lock=>{
 if(!await lock.prepare('SELECT pg_try_advisory_lock(781904303) acquired').first<boolean>('acquired'))throw new Error('ACME_HOSPITALITY_SEED_ALREADY_RUNNING');
 try {
 if(!await one(db,"SELECT id FROM organization_nodes WHERE id='acme-co' AND kind='company'"))throw new Error('ACME_COMPANY_REQUIRED');
 if(!await one(db,"SELECT id FROM organization_nodes WHERE id='bangkok' AND company_id='acme-co' AND kind='branch'"))throw new Error('ACME_BANGKOK_REQUIRED');
 const now=Date.now(),actors:Record<string,WorkContext>={};
        const level=`${namespace}-level`;
        await db.prepare("INSERT INTO staff_levels(id,company_id,code,name,rank,created_at,updated_at) VALUES(?,?,'DEMO','Demo staff',1,?,?) ON CONFLICT DO NOTHING").bind(level,company.id,now,now).run();
        for(const [label,role] of actorRoles){
          const userId=`${namespace}-${label}`,assignmentId=`${userId}-access`,email=`${label.toLowerCase()}@${namespace}.example`;
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
            db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at,level_id) VALUES(?,?,?,?,'employee','salaried','active','2026-01-01',?,'THB',?,?,?,?) ON CONFLICT DO NOTHING").bind(`${userId}-engagement`,`${userId}-person`,company.id,'bangkok',role,userId,now,now,level),
          ]);
          actors[label]={userId,assignmentId,role,companyId:company.id,branchId:null,departmentId:null,holdingId,holdingSlug,holdingName:'ACME',name:label,email,assignmentScope:'holding',inherited:false,modules:[...modules],assignments:[],workspaces:[],scope:{kind:'holding',holdingId,holdingSlug,holdingName:'ACME',companyId:company.id}};
        }

 const admin=actors.administrator,finance=actors.financeApprover;
 const propertyRow=await one(db,"SELECT id FROM pms_properties WHERE company_id='acme-co' AND branch_id='bangkok'");
 if(!propertyRow)throw new Error('ACME_EXISTING_HOTEL_REQUIRED');
 const property=String(propertyRow.id);
 const pms=async(label:string,operation:string,input:unknown={},actor=admin)=>{
  const snapshot=await loadPms(db,admin,property);
  return step(label,{operation,propertyId:property,expectedVersion:snapshot.property.version,input},(body,key)=>pmsCommand(db,actor,body,key));
 };
 let hotel=await loadPms(db,admin,property);
 if(!hotel.property.config.cancellationPolicy)await pms('hotel-policy','property.save',{name:hotel.property.name,country:hotel.property.config.country,timezone:hotel.property.config.timezone,checkIn:hotel.property.config.checkIn,checkOut:hotel.property.config.checkOut,cancellationPolicy:'Fictional Acme rehearsal: free cancellation until the day before arrival.'});
 hotel=await loadPms(db,admin,property);
 if(!hotel.property.config.approvedBy)await pms('finance-approve','accounting.approve',{},finance);
 const types:Record<string,string>={};
 for(const [name,price] of [['Standard',180000],['Deluxe',260000]] as const){
  const type=(await pms(`type:${name}`,'room_type.save',{name,capacity:2,amenities:'Fictional Acme rehearsal room'})).id;types[name]=type;
  await pms(`rate:${name}`,'rate.save',{name:`Acme rehearsal ${name}`,typeId:type,from:plusDays(date,-7),to:plusDays(date,365),price});
 }
 for(const number of ['101','102','103','104','201','202','203','204'])await pms(`room:${number}`,'room.save',{number,typeId:types[number[0]==='1'?'Standard':'Deluxe'],floor:number[0],active:true});
 if((await loadPms(db,admin,property)).property.state==='setup')await pms('hotel-enable','property.enable');
 for(const room of records(await loadPms(db,admin,property),'room').filter(r=>r.data.active&&r.data.cleanliness!=='inspected')){
  const id=(await pms(`task:${room.id}`,'task.create',{roomId:room.id,type:'arrival',date:(await loadPms(db,admin,property)).property.businessDate})).id;
  const checklist=record(await loadPms(db,admin,property),'task',id).data.checklist;
  await pms(`clean:${room.id}`,'task.complete',{id,checked:checklist},actors.housekeeper);
  await pms(`inspect:${room.id}`,'task.inspect',{id},actors.inspector);
 }
 // Advance the untouched old demo business date through its normal audits.
 for(let day=0;day<32;day++){
  const snapshot=await loadPms(db,admin,property),current=snapshot.property.businessDate;
  if(current===date)break;if(current>date)throw new Error('ACME_HOTEL_FUTURE_DATE');
  const night=(await pms(`audit:${current}`,'night.start',{},actors.nightAuditor)).id;
  await drainPms(db,property,night);
  if(record(await loadPms(db,admin,property),'night',night).data.state!=='completed')throw new Error('ACME_AUDIT_BLOCKED');
 }
 if((await loadPms(db,admin,property)).property.businessDate!==date)throw new Error('ACME_DATE_NOT_CURRENT');
 const commerce=(await one(db,"SELECT * FROM commerce_settings WHERE company_id='acme-co'"))!;
 const currency=String(commerce.currency);
 // Preserve the existing Acme ledger. Prices and opening stock use its currency.
 const divisor=currency==='USD'?36:currency==='THB'?1:NaN;
 if(!Number.isFinite(divisor))throw new Error('ACME_CURRENCY_UNSUPPORTED');
 const products:Record<string,string>={};
 for(const stock of fixture.restaurant.openingStock){const menu=fixture.restaurant.menu.find(m=>m.stockSku===stock.sku);
 products[stock.sku]=(await step(`product:${stock.sku}`,{sku:`ACME-HOSP-${stock.sku}`,name:menu?.name??`Acme practice ${stock.sku}`,unit:stock.unit,price:((menu?.priceMinor??100)/100/divisor).toFixed(2),taxBps:menu?700:0},(input,key)=>saveMaster(admin,'products',null,input,key))).id;}
 for(const menu of fixture.restaurant.menu)if(!menu.stockSku)products[menu.sku]=(await step(`menu:${menu.sku}`,{sku:`ACME-HOSP-${menu.sku}`,name:`Acme practice ${menu.name}`,unit:'each',price:(menu.priceMinor/100/divisor).toFixed(2),taxBps:700},(input,key)=>saveMaster(admin,'products',null,input,key))).id;
 const warehouse=(await step('kitchen-warehouse',{code:'ACME-HOSP',name:'Acme hospitality practice kitchen',branchId:'bangkok'},(input,key)=>saveMaster(admin,'warehouses',null,input,key))).id;
 const count=(await step('opening-count',countSchema.parse({warehouseId:warehouse,countDate:date,reason:'Fictional Acme independently counted kitchen opening stock',lines:fixture.restaurant.openingStock.map(s=>({productId:products[s.sku],countedQuantity:String(s.quantityMilli/1000),unitCost:(s.valueMinor/s.quantityMilli*10/divisor).toFixed(2)}))}),(input,key)=>saveCount(admin,null,countSchema.parse(input),key))).id;
 for(const [action,version] of [['submit',1],['approve',2],['post',3]] as const){
  if(state.steps[`stock:${action}`]?.result)continue;
  const who=action==='submit'?admin:finance;
  const flow=await resolveFlow(db,who,'stock_count',count);
  if(!flow.canAct){
   if(!flow.current)throw new Error('ACME_COUNT_ROUTING_REQUIRED');
   await step(`route:${action}`,{stepKey:flow.current.key,userId:who.userId,expectedVersion:flow.version,recordVersion:flow.recordVersion,reason:'Assign fictional Acme stock count to its independent rehearsal reviewer',saveDefault:false},(input,key)=>assignFlowPrimary(db,admin,'stock_count',count,input as Parameters<typeof assignFlowPrimary>[4],key));
  }
  await step(`stock:${action}`,{expectedVersion:version,attachmentSetIds:[],reason:'Independent fictional Acme opening stock review'},(input,key)=>transitionCount(who,count,action,input as {expectedVersion:number;attachmentSetIds:string[];reason:string},key));
 }
 const pos=async(label:string,op:string,input:unknown)=>step(label,input,(body,key)=>posCommand(db,admin,op,body,key));
 const register=(await pos('restaurant-register','register.save',{name:'Acme Practice Restaurant',warehouseId:warehouse,mode:'restaurant'})).id;
 for(const name of fixture.restaurant.tables)await pos(`table:${name}`,'table.create',{registerId:register,name,seats:4});
 for(const menu of fixture.restaurant.menu)await pos(`catalog:${menu.sku}`,'catalog.save',{productId:products[menu.sku],barcode:'',category:'Acme Restaurant',kind:menu.recipe?'recipe':'stock',station:['TEA','COFFEE'].includes(menu.sku)?'Bar':'Kitchen',recipe:menu.recipe?.map(p=>({productId:products[p.sku],quantityMilli:p.quantityMilli}))??[],modifiers:[],taxInclusive:false,expectedVersion:0});
 await writeFile(join(directory,'scenario-guide.json'),JSON.stringify({companyId:'acme-co',branchId:'bangkok',propertyId:property,registerId:register,warehouseId:warehouse,stockCountId:count,pmsCurrency:'THB',posCurrency:currency,date,roomCharging:currency==='THB',setup:`${origin}/holdings/acme/hospitality/setup?branch=bangkok&property=${property}`,hotel:`https://dev-pms.jesadakorn.com/acme/acme-co/branches/bangkok`,restaurant:`https://dev-pos.jesadakorn.com/acme/acme-co/branches/bangkok`},null,2),{mode:0o600});
 console.log(JSON.stringify({companyId:'acme-co',propertyId:property,registerId:register,pmsCurrency:'THB',posCurrency:currency,date,rooms:records(await loadPms(db,admin,property),'room').length,seedSteps:Object.keys(state.steps).length}));
 }finally{await lock.prepare('SELECT pg_advisory_unlock(781904303)').first();}
 });
 await writeFile(join(directory,'credentials.json'),JSON.stringify({origin,users:state.users},null,2),{mode:0o600});
}finally{await db.close();await closeRuntime();}
