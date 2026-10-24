/** Fictional, additive POS rehearsal. Never targets staging or production. */
import {readFile,writeFile,mkdir,rename,access} from 'node:fs/promises';
import {dirname,join,resolve} from 'node:path';
import {createHash,randomBytes} from 'node:crypto';
import {hashPassword} from 'better-auth/crypto';
import {mock,setSystemTime} from 'bun:test';
import {assertHospitalityDevelopmentTarget} from "@erpx/feature-shared/lib/hospitality-seed-target";
import {businessRoles} from "@erpx/feature-identity_access/lib/business-roles";
import {accountKeys,businessDate} from "@erpx/server-pos/public/rules/pos";
import {pmsAccountKeys,plusDays,records,record} from "@erpx/server-pms/public/rules/pms-model";
import {posDemoCompany as company,posDemoBranches as branches,posRestaurantProducts as menus,posRetailProducts as retail,posIngredients,posImagePrompts} from "../../fixtures/pos/catalog";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {PostgresDatabase} from "@erpx/server-runtime/adapters/runtime/postgres";
mock.module('next/navigation',()=>({forbidden:()=>{throw new Error('CLI_PAGE_FORBIDDEN');},redirect:()=>{throw new Error('CLI_REDIRECT_FORBIDDEN');}}));

const mode=process.argv.includes('--apply')?'apply':process.argv.includes('--verify')?'verify':'preview';
const manifestPath=process.env.ERPX_DATABASE_MANIFEST;
if(!manifestPath)throw new Error('ERPX_DATABASE_MANIFEST_REQUIRED');
const manifest=JSON.parse(await readFile(manifestPath,'utf8'));
assertHospitalityDevelopmentTarget(manifest,process.env.ERPX_ENVIRONMENT??process.env.NODE_ENV);
const jobsPath=join(dirname(manifestPath),'databases-jobs.json');
const jobsManifest=JSON.parse(await readFile(jobsPath,'utf8'));
assertHospitalityDevelopmentTarget(jobsManifest,process.env.ERPX_ENVIRONMENT??process.env.NODE_ENV);
const namespace='pos-thb-v1',directory=resolve('.scratch/dev/pos-seed');
const digest=createHash('sha256').update(new URL(jobsManifest.databases.H001_DB).pathname).digest('hex').slice(0,12);
const statePath=join(directory,`state-${digest}.json`);
type Result={id:string;[key:string]:unknown};
type State={date:string;steps:Record<string,{input:unknown;result?:Result}>;users:{email:string;password:string;role:string}[]};
const state:State=await readFile(statePath,'utf8').then(JSON.parse).catch(e=>{if(e.code!=='ENOENT')throw e;return {date:businessDate(),steps:{},users:[]};});
const first=plusDays(state.date,-6),allProducts=[...menus,...retail];
const missingImages:string[]=[];
for(const image of posImagePrompts)if(!await access(resolve(`fixtures/pos/images/${image.id}.png`)).then(()=>true).catch(()=>false))missingImages.push(image.id);
if(mode==='preview'){
 console.log(JSON.stringify({mode,developmentOnly:true,company,branches,currency:'THB',timezone:'Asia/Bangkok',period:[first,state.date],menus:menus.length,retailSkus:retail.length,tables:12,customers:20,completedBills:60,images:posImagePrompts.length,missingImages,completedSeedSteps:Object.values(state.steps).filter(s=>s.result).length,policy:'Add seed-owned records; replay completed commands without overwriting user edits; Stripe acceptance uses real sandbox separately.'},null,2));
 process.exit(0);
}
process.env.ERPX_DATABASE_MANIFEST=jobsPath;process.env.ERPX_PROCESS_ROLE='jobs';
const {getRuntime,closeRuntime}=await import("@erpx/server-runtime/adapters/runtime/env");
const {posCommand}=await import("@erpx/server-pos/public/pos-core");
const {one,all,run}=await import("@erpx/server-runtime/public/runtime/sql");
const {posDayReport}=await import("@erpx/server-pos/public/pos-daily");
const {savePosProductImage}=await import("@erpx/server-pos/public/pos-images");
const {saveMaster}=await import("@erpx/server-commerce/public/commerce");
const {countSchema,saveCount,transitionCount}=await import("@erpx/server-inventory/public/inventory-counts");
const {resolveFlow,assignFlowPrimary}=await import("@erpx/server-workflow/public/flow-routing");
const {confirmManualRate,manualRateSchema}=await import("@erpx/server-finance/public/company-money");
const {importBankStatement}=await import("@erpx/server-finance/public/finance");
const {pmsCommand,loadPms}=await import("../../packages/server-pms/src/public/pms");
const env=getRuntime(),db=new PostgresDatabase({connectionString:jobsManifest.databases.H001_DB,max:3}),control=env.CONTROL_DB;
const roleSpecs=[['admin','holding_owner'],['cashier','pos_cashier'],['kitchen','pos_kitchen'],['manager','pos_manager'],['finance','finance_approver'],['housekeeper','pms_housekeeping'],['inspector','pms_supervisor']] as const;
const actors:Record<string,WorkContext>={};
for(const [label,role] of roleSpecs){const userId=`${namespace}-${label}`,modules=role==='holding_owner'?['pos','pms','finance','inventory','commerce','organization','iam','people']:[...businessRoles[role]];
 actors[label]={userId,assignmentId:`${userId}-access`,role,companyId:company,branchId:null,departmentId:null,holdingId:'h001',holdingSlug:'acme',holdingName:'ACME',name:label,email:`${label}@${namespace}.example`,assignmentScope:'holding',inherited:false,modules,assignments:[],workspaces:[],scope:{kind:'holding',holdingId:'h001',holdingSlug:'acme',holdingName:'ACME',companyId:company}};
}
const admin=actors.admin,finance=actors.finance,manager=actors.manager,cashier=actors.cashier;
async function persist(){await mkdir(directory,{recursive:true,mode:0o700});await writeFile(`${statePath}.tmp`,JSON.stringify(state,null,2),{mode:0o600});await rename(`${statePath}.tmp`,statePath);}
const keyFor=(label:string)=>`${namespace}-${createHash('sha256').update(label).digest('hex').slice(0,32)}`;
async function step(label:string,input:unknown,fn:(input:unknown,key:string)=>Promise<unknown>):Promise<Result>{
 if(state.steps[label]?.result)return state.steps[label].result!;
 if(!state.steps[label]){state.steps[label]={input};await persist();}
 const result=await fn(state.steps[label].input,keyFor(label)) as Result;state.steps[label].result=result;await persist();return result;
}
const pos=(label:string,op:string,input:unknown,actor=admin)=>step(label,input,(body,key)=>posCommand(db,actor,op,body,key));
async function change(label:string,op:string,id:string,extra:Record<string,unknown>={},actor=cashier,table='pos_sales'){
 const row=await one(db,`SELECT version FROM ${table} WHERE id=?`,id);
 if(!row)throw new Error(`SEED_RECORD_MISSING:${label}`);
 return pos(label,op,{id,expectedVersion:row.version,...extra},actor);
}
const chart=`${company}-chart`,accounts=Object.fromEntries(accountKeys.map(k=>[k,`${company}-pos-${k}`])) as Record<string,string>;
const hotelAccounts=Object.fromEntries(pmsAccountKeys.map(k=>[k,['cash','bank','edc','inventory','cashDifference'].includes(k)?accounts[k]:`${company}-pms-${k}`]));
async function verify(){
 const counts=(await one(db,`SELECT (SELECT COUNT(*) FROM pos_catalog WHERE company_id=?) catalog,(SELECT COUNT(*) FROM pos_catalog c JOIN commerce_products p ON p.id=c.product_id WHERE c.company_id=? AND p.sku LIKE 'ING-%') ingredients,(SELECT COUNT(*) FROM pos_tables t JOIN pos_registers r ON r.id=t.register_id WHERE r.company_id=?) tables,(SELECT COUNT(*) FROM finance_parties WHERE company_id=? AND kind='customer') customers,(SELECT COUNT(*) FROM pos_sales WHERE company_id=? AND state='completed') bills,(SELECT COUNT(DISTINCT business_date) FROM pos_sales WHERE company_id=? AND state='completed') days,(SELECT COUNT(*) FROM pos_sale_documents WHERE company_id=?) receipts,(SELECT COUNT(*) FROM pos_day_closes WHERE company_id=? AND state='confirmed') closedDays`,...Array(8).fill(company)))!;
 const unbalanced=await all(db,`SELECT j.id FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.company_id=? AND j.status='posted' GROUP BY j.id HAVING SUM(l.debit_minor)<>SUM(l.credit_minor)`,company);
 const stock=(await one(db,'SELECT COALESCE(SUM(value_minor),0) value FROM commerce_stock WHERE warehouse_id IN (SELECT id FROM commerce_warehouses WHERE company_id=?)',company))!;
 const inventory=(await one(db,"SELECT COALESCE(SUM(l.debit_minor-l.credit_minor),0) value FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.status='posted' AND l.account_id=?",company,accounts.inventory))!;
 const rows=await all(db,'SELECT i.*,c.image_id linked FROM pos_product_images i JOIN pos_catalog c ON c.product_id=i.product_id WHERE i.company_id=?',company);
 const {holdingResources}=await import("../../packages/server-runtime/src/adapters/runtime");const {files}=await holdingResources('h001');let validImages=0;
 for(const image of rows)if(image.linked===image.id&&await files.head(String(image.object_key))&&await files.head(String(image.thumbnail_key)))validImages++;
 const reports=[];for(const branch of Object.values(branches))for(let day=0;day<7;day++)reports.push(await posDayReport(db,admin,branch,plusDays(first,day)));
 const failed:string[]=[];
 if(Number(counts.catalog)!==64||Number(counts.ingredients)!==0)failed.push('catalog scope');
 if(Number(counts.tables)!==12||Number(counts.customers)<20||Number(counts.bills)<60||Number(counts.days)<7||Number(counts.receipts)!==Number(counts.bills))failed.push('fixture completeness');
 if(Number(counts.closedDays)<12)failed.push('historical day closes');
 if(unbalanced.length||Number(stock.value)!==Number(inventory.value))failed.push('ledger / stock reconciliation');
 if(validImages!==64)failed.push('catalog image bytes / thumbnails');
 if(reports.some(r=>r.businessDate<state.date&&r.blockers.length))failed.push('historical close blockers');
 const proof={schema:1,company,currency:'THB',period:[first,state.date],counts,validImages,unbalancedJournals:unbalanced.length,stockValueMinor:Number(stock.value),inventoryLedgerMinor:Number(inventory.value),reports:reports.map(r=>({branchId:r.branchId,businessDate:r.businessDate,totals:r.totals,blockers:r.blockers,digest:r.digest})),stripe:'Not represented by fictional manual history; real sandbox acceptance is reported separately.',failed};
 await mkdir(directory,{recursive:true,mode:0o700});await writeFile(join(directory,'verification.json'),JSON.stringify(proof,null,2),{mode:0o600});
 console.log(JSON.stringify(proof,null,2));if(failed.length)throw new Error(`POS_SEED_VERIFY_FAILED:${failed.join(', ')}`);
}
try{
 if(mode==='verify'){await verify();}else await db.withSession(async lock=>{
 if(!await lock.prepare('SELECT pg_try_advisory_lock(781904365) acquired').first<boolean>('acquired'))throw new Error('POS_SEED_ALREADY_RUNNING');
 try{
 setSystemTime(new Date(`${first}T08:00:00+07:00`));const now=Date.now();
 await run(db,"INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Acme ร้านตัวอย่าง THB',?) ON CONFLICT DO NOTHING",company,company,company);
 for(const [kind,id] of Object.entries(branches))await run(db,"INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,?,?,?,?) ON CONFLICT DO NOTHING",id,id,kind==='restaurant'?'ร้านอาหารและโรงแรมตัวอย่าง':'ร้านค้าปลีกตัวอย่าง',company,company,id);
 const level=`${namespace}-level`;await run(db,"INSERT INTO staff_levels(id,company_id,code,name,rank,created_at,updated_at) VALUES(?,?,'POS-DEMO','พนักงานร้านตัวอย่าง',1,?,?) ON CONFLICT DO NOTHING",level,company,now,now);
 for(const [label,role] of roleSpecs){
  const actor=actors[label];let credential=state.users.find(u=>u.email===actor.email);
  if(!credential){credential={email:actor.email,password:`Practice-${randomBytes(18).toString('base64url')}!`,role};state.users.push(credential);await persist();}
  await control.batch([
   control.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?) ON CONFLICT DO NOTHING').bind(actor.userId,`POS ตัวอย่าง ${label}`,actor.email,now,now),
   control.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?) ON CONFLICT DO NOTHING").bind(`${actor.userId}-account`,actor.userId,actor.userId,await hashPassword(credential.password),now,now),
   control.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?) ON CONFLICT DO NOTHING").bind(`${actor.userId}-principal`,actor.userId,label,now),
  ]);
  await run(db,"INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,?,?,?,0,9999999999999,'holding') ON CONFLICT DO NOTHING",actor.assignmentId,actor.userId,role,JSON.stringify(actor.modules),company);
  await run(db,"INSERT INTO hr_people(id,user_id,company_id,legal_name,email,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'TH',?,?,?) ON CONFLICT DO NOTHING",`${actor.userId}-person`,actor.userId,company,`POS ตัวอย่าง ${label}`,actor.email,admin.userId,now,now);
  await run(db,"INSERT INTO hr_engagements(id,person_id,company_id,branch_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at,level_id) VALUES(?,?,?,?,'employee','salaried','active',?,?,'THB',?,?,?,?) ON CONFLICT DO NOTHING",`${actor.userId}-engagement`,`${actor.userId}-person`,company,branches.restaurant,first,role,admin.userId,now,now,level);
 }
 // Existing Acme holding owners can open the new sample company in their own session.
 for(const owner of await all(db,"SELECT DISTINCT user_id,modules_json FROM assignments WHERE company_id='acme-co' AND role='holding_owner' AND revoked_at IS NULL AND valid_from<=? AND valid_until>?",now,now))await run(db,"INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,'holding_owner',?,?,0,9999999999999,'holding') ON CONFLICT DO NOTHING",`${namespace}-owner-${createHash('sha256').update(String(owner.user_id)).digest('hex').slice(0,16)}`,owner.user_id,owner.modules_json,company);
 await step('default-currency',{},async()=>{await run(db,"UPDATE company_money_settings SET default_currency='THB',updated_by=?,updated_at=? WHERE company_id=?",admin.userId,now,company);return {id:company};});
 await step('fictional-fx',{currency:'THB',effectiveDate:first,currencyPerUsd:'36',note:'Fictional POS rehearsal rate; not market data'},(input,key)=>confirmManualRate(finance,manualRateSchema.parse(input),key));
 await run(db,"INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES(?,?,'ผังบัญชีร้านตัวอย่าง THB','THB',?,?,?) ON CONFLICT DO NOTHING",chart,company,admin.userId,now,now);
 await run(db,"INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES(?,?,'งวดร้านตัวอย่าง',?,?,?) ON CONFLICT DO NOTHING",`${company}-period`,company,`${first.slice(0,4)}-01-01`,`${Number(first.slice(0,4))+1}-12-31`,now);
 const category=(k:string)=>['revenue','service'].includes(k)?'revenue':['outputTax','tax','advance','deposit','refund','payable'].includes(k)?'liability':['cogs','consumption','waste','fees','cashDifference','commission','variance'].includes(k)?'expense':'asset';
 const ledger=new Map<string,string>();for(const [k,id] of Object.entries({...accounts,payable:`${company}-payable`,variance:`${company}-stock-variance`}))ledger.set(id,k);for(const [k,id] of Object.entries(hotelAccounts))ledger.set(id,k);
 for(const [id,k] of ledger)await run(db,"INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',?,?,?,?) ON CONFLICT DO NOTHING",id,chart,company,k==='variance'?'5400':id.replace(`${company}-`,'').toUpperCase(),`ร้านตัวอย่าง ${k}`,category(k),k==='bank'?1:0,k==='bank'?'bank':'none',now,now);
 await step('prepaid-tax-category',{},async()=>{await run(db,"UPDATE finance_accounts SET category='asset',version=version+1 WHERE id=? AND version=1 AND category='liability'",hotelAccounts.prepaidTax);return {id:hotelAccounts.prepaidTax};});
 await run(db,"INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,output_tax_account_id,input_tax_account_id,cash_account_id) VALUES(?,?,'THB',?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING",company,chart,accounts.receivable,`${company}-payable`,accounts.revenue,accounts.inventory,`${company}-payable`,accounts.cogs,accounts.outputTax,hotelAccounts.prepaidTax,accounts.cash);
 await pos('pos-settings','settings.save',{enabled:true,currency:'THB',timezone:'Asia/Bangkok',cashierDiscountBps:500,accounts:{...accounts,chartId:chart},expectedVersion:0});
 await step('pos-accounting-review',{},async(_input,key)=>{const settings=await one(db,'SELECT version FROM pos_settings WHERE company_id=?',company);return posCommand(db,finance,'accounting.approve',{expectedVersion:Number(settings!.version)},key);});
 const products:Record<string,string>={},warehouses:Record<string,string>={},registers:Record<string,string>={},tables:string[]=[],customers:string[]=[];
 for(const [sku,name,unit] of posIngredients)products[sku]=(await step(`product:${sku}`,{sku,name,unit,price:'0',taxBps:0},(input,key)=>saveMaster(admin,'products',null,input,key))).id;
 for(const p of allProducts)products[p.sku]=(await step(`product:${p.sku}`,{sku:p.sku,name:p.name,unit:'each',price:(p.priceMinor/100).toFixed(2),taxBps:700},(input,key)=>saveMaster(admin,'products',null,input,key))).id;
 for(const [kind,branchId] of Object.entries(branches)){
  warehouses[kind]=(await step(`warehouse:${kind}`,{code:`POS-${kind.toUpperCase()}`,name:kind==='restaurant'?'คลังครัวตัวอย่าง':'คลังร้านค้าตัวอย่าง',branchId},(input,key)=>saveMaster(admin,'warehouses',null,input,key))).id;
  const lines=kind==='restaurant'?[...posIngredients.map(([sku,,,qty,cost])=>({productId:products[sku],countedQuantity:String(qty/1000),unitCost:(cost/100).toFixed(2)})),{productId:products['MENU-WATER'],countedQuantity:'600',unitCost:'7'}]:retail.map(p=>({productId:products[p.sku],countedQuantity:'200',unitCost:(Math.round(p.priceMinor*0.55)/100).toFixed(2)}));
  const count=(await step(`count:${kind}`,countSchema.parse({warehouseId:warehouses[kind],countDate:first,reason:'เปิดสต็อกตัวอย่างด้วยราคาทุน ตรวจนับโดยคนละคน',lines}),(input,key)=>saveCount(admin,null,countSchema.parse(input),key))).id;
  for(const [action,version] of [['submit',1],['approve',2],['post',3]] as const){
   const label=`count:${kind}:${action}`;if(state.steps[label]?.result)continue;const who=action==='submit'?admin:finance,flow=await resolveFlow(db,who,'stock_count',count);
   if(!flow.canAct){if(!flow.current)throw new Error('POS_COUNT_ROUTING_REQUIRED');await step(`${label}:route`,{stepKey:flow.current.key,userId:who.userId,expectedVersion:flow.version,recordVersion:flow.recordVersion,reason:'ผู้ตรวจสต็อกตัวอย่างอิสระ',saveDefault:false},(input,key)=>assignFlowPrimary(db,admin,'stock_count',count,input as Parameters<typeof assignFlowPrimary>[4],key));}
   await step(label,{expectedVersion:version,attachmentSetIds:[],reason:'ตรวจและลงบัญชีสต็อกตั้งต้นตัวอย่าง'},(input,key)=>transitionCount(who,count,action,input as Parameters<typeof transitionCount>[3],key));
  }
  registers[kind]=(await pos(`register:${kind}`,'register.save',{name:kind==='restaurant'?'ร้านอาหารตัวอย่าง':'ร้านค้าปลีกตัวอย่าง',warehouseId:warehouses[kind],mode:kind})).id;
 }
 for(let i=1;i<=12;i++)tables.push((await pos(`table:${i}`,'table.create',{registerId:registers.restaurant,name:`โต๊ะ ${String(i).padStart(2,'0')}`,seats:i>8?6:4})).id);
 for(const p of allProducts)await pos(`catalog:${p.sku}`,'catalog.save',{productId:products[p.sku],barcode:p.barcode??'',category:p.category,kind:p.kind,station:p.category==='เครื่องดื่ม'?'บาร์':'ครัว',recipe:p.recipe?.map(part=>({productId:products[part.sku],quantityMilli:part.quantityMilli}))??[],modifiers:p.kind==='recipe'&&p.category==='อาหารจานเดียว'?[{id:'extra-egg',name:'เพิ่มไข่ดาว',priceMinor:1500,ingredients:[{productId:products['ING-EGG'],quantityMilli:1000}]}]:[],taxInclusive:false,enabled:true,registerIds:[registers[p.sku.startsWith('MENU-')?'restaurant':'retail']],expectedVersion:0});
 for(let i=1;i<=20;i++)customers.push((await pos(`customer:${i}`,'customer.create',{name:`ลูกค้าตัวอย่าง ${String(i).padStart(2,'0')}`,phone:`080900${String(i).padStart(4,'0')}`,email:`customer${i}@pos-thb.example`,address:{line1:`${i} ถนนตัวอย่าง`,city:'กรุงเทพมหานคร',postalCode:'10110',countryCode:'TH'}},cashier)).id);
 console.log(JSON.stringify({phase:'setup',company,products:64,tables:12,customers:20}));
 let bill=0;
 for(let day=0;day<7;day++){
  const date=plusDays(first,day);setSystemTime(new Date(`${date}T10:00:00+07:00`));const shifts:Record<string,string>={};
  for(const kind of Object.keys(branches))shifts[kind]=(await pos(`day:${day}:shift:${kind}`,'shift.open',{registerId:registers[kind],openingMinor:200000},cashier)).id;
  for(let i=0;i<(day<4?9:8);i++,bill++){
   const kind=i%2===0?'restaurant':'retail',label=`bill:${bill}`,selected=kind==='restaurant'?[menus[bill%12],menus[12+bill%8]]:[retail[bill%40],retail[(bill+7)%40]];
   const sale=(await pos(label,'sale.create',{shiftId:shifts[kind],serviceMode:kind==='restaurant'?'dine_in':'retail',tableId:kind==='restaurant'?tables[bill%12]:null,partyId:customers[bill%20],guests:1,currency:'THB'},cashier)).id;
   for(const [index,p] of selected.entries())await change(`${label}:line:${index}`,'sale.add',sale,{productId:products[p.sku],quantityMilli:kind==='retail'&&index===0?2000:1000,modifierIds:p.category==='อาหารจานเดียว'&&bill%4===0?['extra-egg']:[],discountMinor:bill%3===0&&index===0?100:0,note:kind==='restaurant'?'ตัวอย่าง: ไม่เผ็ด':''});
   if(bill%5===0)await change(`${label}:discount`,'sale.discount',sale,{amountMinor:200});
   if(bill%6===0){await change(`${label}:hold`,'sale.hold',sale);await change(`${label}:resume`,'sale.resume',sale);}
   for(const f of await all(db,"SELECT f.id FROM pos_fulfillments f JOIN pos_lines l ON l.fulfillment_id=f.id WHERE l.sale_id=? AND f.kind='recipe' ORDER BY f.id",sale))for(const action of ['send','start','ready','deliver'])await change(`${label}:kitchen:${f.id}:${action}`,`kitchen.${action}`,String(f.id),{},actors.kitchen,'pos_fulfillments');
   const amounts=(await one(db,'SELECT total_minor,discount_minor FROM pos_sales WHERE id=?',sale))!;
   if(Number(amounts.discount_minor)>0)await change(`${label}:approve-discount`,'sale.approve_discount',sale,{reason:'ผู้จัดการอีกคนอนุมัติส่วนลดตัวอย่าง'},manager);
   const total=Number(amounts.total_minor),tender=bill%4;
   if(tender===2){
    const ref=`POS-DEMO-${bill}`,payment=(await change(`${label}:payment`,'payment.create',sale,{method:'transfer',amountMinor:total,tenderedMinor:total,reference:ref})).id;
    await step(`${label}:bank`,{bankAccount:accounts.bank,statementNumber:ref,statementDate:date,opening:'0',closing:(total/100).toFixed(2),sourceHash:keyFor(`${label}:bank`),rows:[{externalId:ref,transactionDate:date,description:'Fictional POS sample transfer evidence',amount:(total/100).toFixed(2),currency:'THB'}]},(input,key)=>importBankStatement(admin,input as Parameters<typeof importBankStatement>[1],key));
    const bank=(await one(db,'SELECT id,version FROM finance_bank_transactions WHERE company_id=? AND external_id=?',company,ref))!;
    await change(`${label}:verify-transfer`,'payment.verify_transfer',payment,{bankTransactionId:bank.id,bankVersion:bank.version},finance,'pos_payments');
   }else if(tender===1){const cash=Math.floor(total/2);await change(`${label}:cash`,'payment.create',sale,{method:'cash',amountMinor:cash,tenderedMinor:cash+10000,reference:''});await change(`${label}:edc`,'payment.create',sale,{method:'edc',amountMinor:total-cash,tenderedMinor:total-cash,reference:`FICTIONAL-EDC-${bill}`});}
   else await change(`${label}:cash`,'payment.create',sale,{method:'cash',amountMinor:total,tenderedMinor:Math.ceil(total/10000)*10000,reference:''});
   await change(`${label}:complete`,'sale.complete',sale);
   if(kind==='retail'&&i===1){const line=(await one(db,'SELECT id FROM pos_lines WHERE sale_id=? AND quantity_milli=2000',sale))!;
    const refund=(await change(`${label}:refund`,'refund.request',sale,{reason:'คืนสินค้าตัวอย่างบางส่วน สภาพพร้อมขาย',payoutShiftId:shifts.retail,lines:[{lineId:line.id,quantityMilli:1000,restock:true}]})).id;
    await change(`${label}:refund-approve`,'refund.approve',refund,{reason:'ผู้จัดการตรวจสินค้าคืนและยืนยันการคืนเงิน',attachmentSetIds:[]},manager,'pos_refunds');
   }
  }
  // Cancel after cooking: cost moves from work in progress to waste once.
  const waste=(await pos(`day:${day}:waste`,'sale.create',{shiftId:shifts.restaurant,serviceMode:'takeaway',currency:'THB'},cashier)).id;
  await change(`day:${day}:waste-line`,'sale.add',waste,{productId:products[menus[0].sku],quantityMilli:1000,modifierIds:[],discountMinor:0,note:'ตัวอย่างของเสีย'});
  const f=(await one(db,'SELECT fulfillment_id FROM pos_lines WHERE sale_id=?',waste))!;
  for(const action of ['send','start'])await change(`day:${day}:waste-${action}`,`kitchen.${action}`,String(f.fulfillment_id),{},actors.kitchen,'pos_fulfillments');
  await change(`day:${day}:waste-cancel`,'kitchen.cancel',String(f.fulfillment_id),{reason:'ผู้จัดการอีกคนยืนยันอาหารเสียหลังเริ่มทำ'},manager,'pos_fulfillments');
  await change(`day:${day}:waste-bill-cancel`,'sale.cancel',waste,{reason:'ยกเลิกบิลตัวอย่างหลังบันทึกของเสีย'});
  for(const kind of Object.keys(branches)){
   const movement=(await change(`day:${day}:${kind}:cash`,'cash.create',shifts[kind],{amountMinor:day%2? -5000:5000,reason:'เงินเข้าออกลิ้นชักตัวอย่าง'},cashier,'pos_shifts')).id;
   await change(`day:${day}:${kind}:cash-approve`,'cash.approve',shifts[kind],{movementId:movement},manager,'pos_shifts');
   const sum=(await one(db,`SELECT s.opening_minor+(SELECT COALESCE(SUM(p.amount_minor),0) FROM pos_payments p JOIN pos_sales b ON b.id=p.sale_id WHERE b.shift_id=s.id AND p.method='cash' AND p.state='succeeded')-(SELECT COALESCE(SUM(rp.amount_minor),0) FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id JOIN pos_payments p ON p.id=rp.payment_id WHERE r.payout_shift_id=s.id AND p.method='cash' AND rp.state='succeeded')+(SELECT COALESCE(SUM(amount_minor),0) FROM pos_cash_movements WHERE shift_id=s.id AND approved_by IS NOT NULL) expected FROM pos_shifts s WHERE s.id=?`,shifts[kind]))!;
   await change(`day:${day}:${kind}:close`,'shift.close',shifts[kind],{countedMinor:Number(sum.expected)+(day%3===0?100:day%3===1?-200:0)},cashier,'pos_shifts');
   await change(`day:${day}:${kind}:review`,'shift.review',shifts[kind],{reason:'ผู้จัดการอีกคนตรวจเงินสดและลงบัญชีเฉพาะผลต่าง'},manager,'pos_shifts');
   if(day<6){const close=(await pos(`day:${day}:${kind}:prepare`,'day.prepare',{branchId:branches[kind as keyof typeof branches],businessDate:date},manager)).id;await change(`day:${day}:${kind}:confirm`,'day.confirm',close,{},finance,'pos_day_closes');}
  }
  console.log(JSON.stringify({phase:'history',businessDate:date,completedBills:bill}));
 }
 setSystemTime(new Date(`${state.date}T16:00:00+07:00`));
 const property=(await step('hotel',{operation:'property.create',input:{name:'โรงแรมร้านอาหารตัวอย่าง',branchId:branches.restaurant,businessDate:state.date}},(input,key)=>pmsCommand(db,admin,input,key))).id;
 const pms=async(label:string,operation:string,input:unknown={},who=admin)=>step(`hotel:${label}`,{operation,propertyId:property,expectedVersion:(await loadPms(db,admin,property)).property.version,input},(body,key)=>pmsCommand(db,who,body,key));
 await pms('policy','property.save',{name:'โรงแรมร้านอาหารตัวอย่าง',country:'TH',timezone:'Asia/Bangkok',checkIn:'14:00',checkOut:'12:00',cancellationPolicy:'นโยบายตัวอย่าง: ยกเลิกฟรีก่อนวันเข้าพัก'});
 await pms('accounts','accounting.bootstrap',{chartId:chart,accounts:hotelAccounts,taxBps:700,serviceBps:0,taxInclusive:false,advanceTax:true,financeReviewed:true});
 await pms('approve','accounting.approve',{},finance);
 const type=(await pms('type','room_type.save',{name:'ห้องตัวอย่าง',capacity:2,amenities:'ห้องพักสำหรับทดสอบ POS → PMS'})).id;
 const rate=(await pms('rate','rate.save',{name:'ราคาห้องตัวอย่าง',typeId:type,from:state.date,to:plusDays(state.date,365),price:150000})).id;
 for(let i=1;i<=4;i++)await pms(`room:${i}`,'room.save',{number:`10${i}`,typeId:type,floor:'1',active:true});
 await pms('enable','property.enable');
 for(const room of records(await loadPms(db,admin,property),'room')){const task=(await pms(`task:${room.id}`,'task.create',{roomId:room.id,type:'arrival',date:state.date})).id;const checklist=record(await loadPms(db,admin,property),'task',task).data.checklist;await pms(`clean:${room.id}`,'task.complete',{id:task,checked:checklist},actors.housekeeper);await pms(`inspect:${room.id}`,'task.inspect',{id:task},actors.inspector);}
 const guest=(await pms('guest','guest.save',{name:'ผู้เข้าพักตัวอย่าง POS'})).id;
 const booking=await pms('booking','reservation.create',{guestId:guest,source:'staff',rooms:[{typeId:type,rateId:rate,arrival:state.date,departure:plusDays(state.date,2),adults:2,quantity:1}]});
 const stay=records(await loadPms(db,admin,property),'stay').find(s=>s.data.reservationId===booking.id)!;
 const room=records(await loadPms(db,admin,property),'room').find(r=>r.data.number==='101')!;
 await pms('assign','stay.assign',{id:stay.id,roomId:room.id});
 await pms('check-in','stay.check_in',{id:stay.id});
 for(const p of allProducts){const path=resolve(`fixtures/pos/images/${p.image}.png`);if(!await access(path).then(()=>true).catch(()=>false))continue;
  await step(`image:${p.sku}`,{productId:products[p.sku],path,expectedVersion:1},async(input,key)=>{const args=input as {productId:string;path:string;expectedVersion:number};return savePosProductImage(admin,args.productId,new Uint8Array(await readFile(args.path)),'image/png','generated',key,args.expectedVersion,{fixture:p.image,prompt:posImagePrompts.find(image=>image.id===p.image)!.prompt,generatedFor:'POS fictional dev catalog'});});
 }
 await persist();await writeFile(join(directory,'credentials.json'),JSON.stringify({origin:'https://console-dev-erpx.jesadakorn.com',company,users:state.users},null,2),{mode:0o600});
 await writeFile(join(directory,'scenario-guide.json'),JSON.stringify({company,branches,propertyId:property,stayId:stay.id,registers,warehouses,currency:'THB',date:state.date,restaurant:`https://dev-pos.jesadakorn.com/acme/${company}/branches/${branches.restaurant}`,retail:`https://dev-pos.jesadakorn.com/acme/${company}/branches/${branches.retail}`},null,2),{mode:0o600});
 await verify();
 }finally{setSystemTime();await lock.prepare('SELECT pg_advisory_unlock(781904365)').first();}
 });
}finally{setSystemTime();await db.close();await closeRuntime();}
