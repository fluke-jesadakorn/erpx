import { mkdir,readFile,writeFile } from 'node:fs/promises';
import { appendFileSync } from 'node:fs';
import { createHash,randomUUID } from 'node:crypto';
import { resolve,join } from 'node:path';
import { chromium,type BrowserContext } from '@playwright/test';
import { Pool } from 'pg';
import { runtimeSourceFingerprint,verificationSourceFingerprint } from './source-fingerprint';

// Compile for pinned Node and run only against a freshly prepared release
// cohort. Authentication/selection are normal HTTP flows, never test tokens.
const root=process.cwd(),run=process.argv[2],output=resolve(process.argv[3]??`.scratch/dev/pms-scale-${run}`);
if(process.version!=='v24.20.0'||!run||!/^[a-f0-9]{12}$/.test(run)||!output.startsWith(join(root,'.scratch/dev/')))throw new Error('EXACT_PINNED_PMS_PERFORMANCE_SCOPE_REQUIRED');
const directory=join(root,'.runtime/browser',run),state=JSON.parse(await readFile(join(directory,'state.json'),'utf8'));
if(state.run!==run||state.prefix!==`erpx_browser_${run}`||state.port!==3104||state.profile!=='booking'||![4,8,16].includes(state.applicationPoolMax)||![5,8].includes(state.controlPoolMax)||!state.release||!state.pid||!state.jobsPid)throw new Error('COMPILED_PMS_COHORT_REQUIRED');
const sourceFingerprint=await runtimeSourceFingerprint(root),verificationFingerprint=await verificationSourceFingerprint(root,sourceFingerprint);
if(sourceFingerprint!==state.sourceFingerprint)throw new Error('PMS_PERFORMANCE_SOURCE_CHANGED');
const config=JSON.parse(await readFile(join(directory,'pms-scale.json'),'utf8')) as {
 companyId:string;holdingSlug:string;businessDate:string;historyDays:number;concurrentAccounts:number;password:string;
 properties:Array<{id:string;typeId:string;rateId:string}>;users:Array<{userId:string;email:string;role:string;assignmentId:string}>;
};
if(config.properties.length!==3||config.historyDays!==730||config.concurrentAccounts!==20||config.users.length!==20||new Set(config.users.map(u=>u.userId)).size!==20)throw new Error('EXACT_PMS_SCALE_FIXTURE_REQUIRED');
const manifest=JSON.parse(await readFile(join(directory,'databases.json'),'utf8')),url=new URL(manifest.databases.H001_DB);
if(url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!==`/erpx_browser_${run}_h001_db`)throw new Error('EXACT_OWNED_PMS_DATABASE_REQUIRED');
const db=new Pool({connectionString:url.href,max:1});
await mkdir(output,{recursive:true,mode:0o700});
const raw=join(output,'samples.jsonl');await writeFile(raw,'',{mode:0o600});
const financialEnabled=process.env.ERPX_PMS_FINANCIAL_PERFORMANCE==='1';
if(process.env.ERPX_PMS_FINANCIAL_PERFORMANCE&&!financialEnabled)throw new Error('INVALID_PMS_FINANCIAL_PERFORMANCE_MODE');
const operationalEnabled=process.env.ERPX_PMS_OPERATIONAL_PERFORMANCE==='1';
if(process.env.ERPX_PMS_OPERATIONAL_PERFORMANCE&&!operationalEnabled)throw new Error('INVALID_PMS_OPERATIONAL_PERFORMANCE_MODE');
const closingEnabled=process.env.ERPX_PMS_CLOSING_PERFORMANCE==='1';
if((process.env.ERPX_PMS_CLOSING_PERFORMANCE&&!closingEnabled)||(closingEnabled&&(!operationalEnabled||!financialEnabled)))throw new Error('INVALID_PMS_CLOSING_PERFORMANCE_MODE');
let stage:'core'|'cashier'|'operational'|'closing'='core',financialReconciliation:unknown=null,operationalReconciliation:unknown=null,closingReconciliation:unknown=null;
type Sample={stage?:'core'|'cashier'|'operational'|'closing';account:number;kind:'read'|'write';operation:string;elapsedMs:number;status:number;valid:boolean;attempts?:number;errorCode?:string;replayStatus?:number;replayErrorCode?:string;replayIdMatches?:boolean;replayVersionMatches?:boolean;caughtErrorName?:string;initialResultValid?:boolean;responseStage?:'initial'|'replay'};
const samples:Sample[]=[],contexts:Array<{context:BrowserContext;headers:Record<string,string>;guestId?:string}>=[];
const record=(sample:Sample)=>{const measured={...sample,stage};samples.push(measured);appendFileSync(raw,JSON.stringify(measured)+'\n');};
const base=`/api/v1/holdings/${config.holdingSlug}/pms`,day=(offset:number)=>new Date(Date.parse(config.businessDate)+offset*86400000).toISOString().slice(0,10);
function alive(pid:number){try{process.kill(pid,0);return true;}catch{return false;}}
async function read(account:number,view:string,measured=true,propertyAccount=account){
 const p=config.properties[propertyAccount%3],query=view==='calendar'?`view=calendar&from=${day(7)}&to=${day(10)}`:view==='dashboard'?'view=dashboard':`view=list&section=${view}`;
 const begin=performance.now();let status=0,valid=false,data;
 try{const response=await contexts[account].context.request.get(`${base}?property=${p.id}&${query}`);status=response.status();data=(await response.json()).data;
  valid=status===200&&data?.snapshot?.property?.id===p.id&&Array.isArray(data.snapshot.records);
  if(valid&&view==='rooms')valid=data.snapshot.records.filter((r:{kind:string})=>r.kind==='room').length===50;
 }catch{/* Keep failed samples; never substitute cached data. */}
 if(measured)record({account,kind:'read',operation:view,elapsedMs:performance.now()-begin,status,valid});
 if(!valid)throw new Error(`PMS_PERFORMANCE_READ_INVALID:${view}:${status}`);return data;
}
async function report(account:number){
 const begin=performance.now();let status=0,valid=false;
 try{const response=await contexts[account].context.request.get(`${base}/reports?property=${config.properties[account%3].id}&from=${day(-7)}&to=${day(-1)}`);status=response.status();const data=(await response.json()).data;valid=status===200&&data?.days?.length===7&&data?.totals?.capacity===1400;}
 catch{/* Retain the report failure in raw evidence. */}
 record({account,kind:'read',operation:'seven-day-report',elapsedMs:performance.now()-begin,status,valid});
 if(!valid)throw new Error(`PMS_PERFORMANCE_REPORT_INVALID:${status}`);
}
async function command(account:number,operation:string,input:unknown,key=randomUUID(),propertyAccount=account){
 const begin=performance.now();let status=0,valid=false,result,attempts=0,expectedVersion:number|undefined,errorCode:string|undefined;let replayStatus:number|undefined,replayErrorCode:string|undefined,replayIdMatches:boolean|undefined,replayVersionMatches:boolean|undefined,caughtErrorName:string|undefined;let initialResultValid:boolean|undefined,responseStage:'initial'|'replay'='initial';const p=config.properties[propertyAccount%3];
 try{for(;attempts<20;attempts++){
  expectedVersion??=(await read(account,'dashboard',false,propertyAccount)).snapshot.property.version;
  const body={operation,propertyId:p.id,expectedVersion,input};
  const response=await contexts[account].context.request.post(base,{headers:{...contexts[account].headers,Origin:state.origin,'Idempotency-Key':key},data:body});status=response.status();const value=await response.json();
  errorCode=value.error?.code;
  if(status===409&&errorCode==='PMS_VERSION_CONFLICT'){
   const version=value.error?.details?.currentVersion;
   if(!Number.isSafeInteger(version)||version<=0)throw new Error('PMS_AUTHORITATIVE_CONFLICT_VERSION_REQUIRED');
   expectedVersion=version;continue;
  }
  if(status===409&&errorCode==='PMS_RETRYABLE_CONFLICT'){expectedVersion=undefined;continue;}
  valid=status===200&&value.data?.propertyId===p.id&&typeof value.data?.id==='string';initialResultValid=valid;result=value.data;
  if(valid&&(operation==='reservation.create'||stage!=='core')){
   // Replay the original envelope after its version became stale: same result,
   // same original quote and no duplicate room/account/money snapshot.
   responseStage='replay';const replay=await contexts[account].context.request.post(base,{headers:{...contexts[account].headers,Origin:state.origin,'Idempotency-Key':key},data:body});
   replayStatus=replay.status();const repeated=await replay.json();replayErrorCode=typeof repeated.error?.code==='string'&&/^[A-Z][A-Z0-9_]{0,100}$/.test(repeated.error.code)?repeated.error.code:undefined;replayIdMatches=repeated.data?.id===result.id;replayVersionMatches=repeated.data?.version===result.version;valid=replay.status()===200&&repeated.data?.id===result.id&&repeated.data?.version===result.version;
  }
  break;
 }}catch(error){/* Record names only; never emit body, credentials or identifiers. */caughtErrorName=error instanceof Error&&['Error','TypeError','SyntaxError','TimeoutError','AbortError'].includes(error.name)?error.name:'OtherError';}
 record({account,kind:'write',operation,elapsedMs:performance.now()-begin,status,valid,attempts:attempts+1,...(errorCode?{errorCode}:{}),...(replayStatus!==undefined?{replayStatus,replayIdMatches,replayVersionMatches}:{}),...(replayErrorCode?{replayErrorCode}:{}),...(caughtErrorName?{caughtErrorName}:{}),...(initialResultValid!==undefined?{initialResultValid}:{}),responseStage});
 if(!valid)throw new Error(`PMS_PERFORMANCE_WRITE_INVALID:${operation}:${status}`);return result;
}
const browser=await chromium.launch(),started=Date.now(),counts:unknown[]=[];let passed=false;
try{
 for(const p of config.properties){const {rows}=await db.query("SELECT count(*) FILTER(WHERE kind='room')::int rooms,count(*) FILTER(WHERE kind='stay' AND data_json::jsonb->>'state'='checked_out')::int archived_stays,count(*) FILTER(WHERE kind='reservation')::int reservations,count(*) FILTER(WHERE kind='guest_account' AND data_json::jsonb->>'state'='closed')::int archived_accounts FROM pms_records WHERE property_id=$1",[p.id]);
  if(rows[0].rooms!==200||rows[0].archived_stays!==73000||rows[0].reservations!==73000||rows[0].archived_accounts!==73000)throw new Error('PMS_PERFORMANCE_DATASET_CHANGED');counts.push(rows[0]);}
 for(const [account,user] of config.users.entries()){
  const context=await browser.newContext({baseURL:state.origin});contexts.push({context,headers:{}});
  const login=await context.request.post('/api/auth/sign-in/email',{headers:{Origin:state.origin},data:{email:user.email,password:config.password}});
  if(login.status()!==200)throw new Error(`PMS_PERFORMANCE_NORMAL_LOGIN_FAILED:${login.status()}`);
  const endpoint=`/api/v1/holdings/${config.holdingSlug}/assignments/active`,active=(await(await context.request.get(endpoint)).json()).data;
  if(!active?.options?.some((o:{id:string;role:string})=>o.id===user.assignmentId&&o.role===user.role))throw new Error('PMS_PERFORMANCE_NORMAL_ASSIGNMENT_REQUIRED');
  const select=await context.request.post(endpoint,{headers:{Origin:state.origin},data:{assignmentId:user.assignmentId,revision:active.revision}});
  if(select.status()!==200)throw new Error('PMS_PERFORMANCE_ASSIGNMENT_FAILED');
  const page=await context.newPage();await page.goto(`/holdings/${config.holdingSlug}/home`);
  const scope=await page.locator('[data-work-revision]').first().evaluate(node=>({revision:(node as HTMLElement).dataset.workRevision!,company:(node as HTMLElement).dataset.workCompany!,assignment:(node as HTMLElement).dataset.workAssignment!}));
  if(scope.company!==config.companyId||scope.assignment!==user.assignmentId||!scope.revision)throw new Error('PMS_PERFORMANCE_CURRENT_SCOPE_REQUIRED');
  contexts[account].headers={'x-erpx-context-revision':scope.revision,'x-erpx-context-company':scope.company,'x-erpx-context-assignment':scope.assignment};await page.close();
 }
 config.password='';console.log(JSON.stringify({event:'pms_scale_started',accounts:20,properties:3,roomsPerProperty:200,historyDays:730,archivedStays:219000}));
 await Promise.all(contexts.map((_,a)=>read(a,'rooms',false)));
 for(let round=0;round<10;round++){
  const outcomes=await Promise.allSettled(contexts.map(async(_,a)=>{
   for(const view of ['dashboard','rooms','calendar','guest-accounts'])await read(a,view);
   if(a>=16)await report(a);
   const guest=await command(a,'guest.save',{...(contexts[a].guestId?{id:contexts[a].guestId}:{}),name:`Fictional load guest ${a}`,email:`load-${a}@example.test`,phone:'020000000'});contexts[a].guestId=guest.id;
   const p=config.properties[a%3];await command(a,'reservation.create',{guestId:guest.id,source:'direct',externalRef:`scale-${a}-${round}`,rooms:[{typeId:p.typeId,rateId:p.rateId,arrival:day(7),departure:day(9),adults:2,childAges:[7],quantity:1}]});
  }));
  const failed=outcomes.find(outcome=>outcome.status==='rejected');
  if(failed?.status==='rejected')throw failed.reason;
  console.log(JSON.stringify({event:'pms_scale_round',round:round+1,samples:samples.length}));
 }
 const reservations=await db.query("SELECT count(*)::int n FROM pms_records WHERE property_id=ANY($1::text[]) AND kind='reservation' AND data_json::jsonb->>'externalRef' ~ '^scale-[0-9]+-[0-9]+$'",[config.properties.map(p=>p.id)]);
 const money=await db.query("SELECT count(*)::int n,sum(amount_units::numeric)::text total FROM money_transaction_snapshots WHERE company_id=$1 AND source_type='pms_reservation' AND amount_kind='room_total'",[config.companyId]);
 if(reservations.rows[0].n!==200||money.rows[0].n!==200||money.rows[0].total!=='141240000')throw new Error('PMS_PERFORMANCE_REPLAY_RECONCILIATION_FAILED');
 if(financialEnabled){
  stage='cashier';
  const accountRows=await db.query(`SELECT f.id,f.property_id FROM pms_records r
    JOIN pms_records f ON f.property_id=r.property_id AND f.kind='guest_account' AND f.data_json::jsonb->>'reservationId'=r.id
    WHERE r.property_id=ANY($1::text[]) AND r.kind='reservation' AND r.data_json::jsonb->>'externalRef' ~ '^scale-[0-9]+-9$'
    ORDER BY (substring(r.data_json::jsonb->>'externalRef' FROM '^scale-([0-9]+)-9$'))::int`,[config.properties.map(p=>p.id)]);
  const targets=accountRows.rows as Array<{id:string;property_id:string}>;
  if(targets.length!==20||new Set(targets.map(f=>f.id)).size!==20||targets.some((f,a)=>f.property_id!==config.properties[a%3].id))throw new Error('EXACT_PMS_FINANCIAL_ACCOUNTS_REQUIRED');
  const shifts:string[]=[],refunds:string[]=[];
  async function phase(label:string,work:(account:number)=>Promise<void>){
   const outcomes=await Promise.allSettled(contexts.map((_,a)=>work(a))),failed=outcomes.find(o=>o.status==='rejected');
   if(failed?.status==='rejected')throw failed.reason;
   console.log(JSON.stringify({event:'pms_financial_phase',phase:label,accounts:20}));
  }
  const reviewer=(a:number)=>{const selected=16+(a%4);return selected===a?16+((a+1)%4):selected;};
  await phase('cash-shifts',async a=>{shifts[a]=(await command(a,'shift.open',{name:`Fictional load cashier ${a}`,opening:0})).id;});
  await phase('guest-charges',async a=>{await command(a,'charge.add',{guestAccountId:targets[a].id,price:10000,description:'Fictional load hotel service'});});
  await phase('charge-settlement',async a=>{await command(a,'payment.receive',{guestAccountId:targets[a].id,amount:11770,method:'cash',shiftId:shifts[a]});});
  await phase('room-advances',async a=>{await command(a,'payment.receive',{guestAccountId:targets[a].id,amount:10000,method:'cash',shiftId:shifts[a]});});
  await phase('security-deposits',async a=>{await command(a,'payment.deposit',{guestAccountId:targets[a].id,amount:3000,method:'cash',shiftId:shifts[a]});});
  await phase('partial-refund-requests',async a=>{refunds[a]=(await command(a,'refund.request',{guestAccountId:targets[a].id,amount:1000,pocket:'deposit',reason:'Fictional partial deposit return'})).id;});
  // A distinct normal manager reviews each request, including manager-owned shifts.
  // Keep each manager's work in order rather than submitting concurrent commands
  // through one staff session; four independent managers remain concurrent.
  await Promise.all([16,17,18,19].map(async manager=>{for(let a=0;a<20;a++)if(reviewer(a)===manager)await command(manager,'refund.approve',{id:refunds[a]},randomUUID(),a);}));
  await phase('partial-refund-payouts',async a=>{await command(a,'refund.pay',{id:refunds[a],method:'cash',shiftId:shifts[a]});});
  await phase('cash-shift-close',async a=>{await command(a,'shift.close',{id:shifts[a],counted:23770,handover:'Fictional Finance review'});});
  await Promise.all([16,17,18,19].map(async manager=>{for(let a=0;a<20;a++)if(reviewer(a)===manager)await command(manager,'shift.review',{id:shifts[a]},randomUUID(),a);}));
  const balances=await db.query(`SELECT count(*)::int accounts,sum((data_json::jsonb->>'due')::bigint)::text due,
    sum((data_json::jsonb->>'advance')::bigint)::text advance,sum((data_json::jsonb->>'deposit')::bigint)::text deposit
    FROM pms_records WHERE property_id=ANY($1::text[]) AND kind='guest_account' AND id=ANY($2::text[])`,[config.properties.map(p=>p.id),targets.map(f=>f.id)]);
  const cash=await db.query(`SELECT count(*)::int shifts,sum((data_json::jsonb->>'cash')::bigint)::text cash,
    bool_and(data_json::jsonb->>'state'='reviewed' AND (data_json::jsonb->>'variance')::bigint=0)::boolean reviewed
    FROM pms_records WHERE property_id=ANY($1::text[]) AND kind='shift' AND id=ANY($2::text[])`,[config.properties.map(p=>p.id),shifts]);
  const receipts=await db.query(`SELECT count(*) FILTER(WHERE p.kind='receipt')::int receipts,count(*) FILTER(WHERE p.kind='refund')::int refunds,
    sum(p.amount_minor) FILTER(WHERE p.kind='receipt')::text received,sum(p.amount_minor) FILTER(WHERE p.kind='refund')::text refunded,
    bool_and(p.status='settled')::boolean settled FROM pms_documents d JOIN finance_payments p ON p.id=d.finance_id AND p.company_id=$1
    WHERE d.property_id=ANY($2::text[]) AND d.guest_account_id=ANY($3::text[])`,[config.companyId,config.properties.map(p=>p.id),targets.map(f=>f.id)]);
  const journals=await db.query(`SELECT sum(l.debit_minor)::text debit,sum(l.credit_minor)::text credit
    FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id AND j.company_id=$1
    WHERE j.source_type='pms' AND j.source_id=ANY($2::text[])`,[config.companyId,config.properties.map(p=>p.id)]);
  if(balances.rows[0].accounts!==20||balances.rows[0].due!=='0'||balances.rows[0].advance!=='200000'||balances.rows[0].deposit!=='40000'
    ||cash.rows[0].shifts!==20||cash.rows[0].cash!=='475400'||cash.rows[0].reviewed!==true
    ||receipts.rows[0].receipts!==60||receipts.rows[0].refunds!==20||receipts.rows[0].received!=='495400'||receipts.rows[0].refunded!=='20000'||receipts.rows[0].settled!==true
    ||!journals.rows[0].debit||journals.rows[0].debit!==journals.rows[0].credit)throw new Error('PMS_FINANCIAL_REPLAY_RECONCILIATION_FAILED');
  const history=await db.query(`SELECT count(*)::int records,sum(version)::text versions FROM pms_records
    WHERE property_id=ANY($1::text[]) AND kind IN ('reservation','stay','guest_account') AND id LIKE 'scale-history-%'`,[config.properties.map(p=>p.id)]);
  if(history.rows[0].records!==657000||history.rows[0].versions!=='657000')throw new Error('PMS_FINANCIAL_ARCHIVED_HISTORY_CHANGED');
  financialReconciliation={history:history.rows[0],balances:balances.rows[0],cash:cash.rows[0],receipts:receipts.rows[0],journals:journals.rows[0]};
 }
 if(operationalEnabled){
  stage='operational';
  const stays:string[]=[],accounts:string[]=[],shifts:string[]=[],assignedRooms:string[]=[],movedRooms:string[]=[];
  const roomLists=await Promise.all(config.properties.map(async p=>{
   const rows=(await db.query("SELECT id,data_json::jsonb->>'cleanliness' cleanliness FROM pms_records WHERE property_id=$1 AND kind='room' ORDER BY id",[p.id])).rows;
   if(rows.length!==200)throw new Error('EXACT_PMS_OPERATIONAL_ROOM_SET_REQUIRED');
   return rows.filter(r=>r.cleanliness==='inspected').map(r=>String(r.id));
  }));
  if(roomLists.some(rooms=>rooms.length<107))throw new Error('PMS_OPERATIONAL_INSPECTED_ROOMS_REQUIRED');
  async function phase(label:string,work:(account:number)=>Promise<void>){
   const outcomes=await Promise.allSettled(contexts.map((_,a)=>work(a))),failed=outcomes.find(o=>o.status==='rejected');
   if(failed?.status==='rejected')throw failed.reason;
   console.log(JSON.stringify({event:'pms_operational_phase',phase:label,accounts:20}));
  }
  await phase('arrival-bookings',async a=>{
   const p=config.properties[a%3],booking=await command(a,'reservation.create',{guestId:contexts[a].guestId,source:'direct',externalRef:`operation-${a}`,rooms:[{typeId:p.typeId,rateId:p.rateId,arrival:day(0),departure:day(1),adults:2,childAges:[7],quantity:1}]});
   const rows=await db.query("SELECT id,data_json::jsonb->>'guestAccountId' account_id FROM pms_records WHERE property_id=$1 AND kind='stay' AND data_json::jsonb->>'reservationId'=$2",[p.id,booking.id]);
   if(rows.rows.length!==1)throw new Error('EXACT_PMS_OPERATIONAL_STAY_REQUIRED');
   stays[a]=rows.rows[0].id;accounts[a]=rows.rows[0].account_id;assignedRooms[a]=roomLists[a%3][Math.floor(a/3)];movedRooms[a]=roomLists[a%3][100+Math.floor(a/3)];
  });
  await phase('room-assignment',async a=>{await command(a,'stay.assign',{id:stays[a],roomId:assignedRooms[a]});});
  await phase('check-in',async a=>{await command(a,'stay.check_in',{id:stays[a]});});
  await phase('room-move',async a=>{await command(a,'stay.move',{id:stays[a],roomId:movedRooms[a],reason:'Fictional ready replacement room'});});
  await phase('stay-extension',async a=>{await command(a,'stay.change_dates',{id:stays[a],arrival:day(0),departure:day(2),rateId:config.properties[a%3].rateId,reason:'Fictional extension'});});
  await phase('checkout-preview',async a=>{await command(a,'stay.preview_checkout',{id:stays[a]});});
  await phase('cash-shifts',async a=>{shifts[a]=(await command(a,'shift.open',{name:`Fictional front desk cashier ${a}`,opening:0})).id;});
  await phase('room-settlement',async a=>{await command(a,'payment.receive',{guestAccountId:accounts[a],amount:353100,method:'cash',shiftId:shifts[a]});});
  await phase('early-checkout',async a=>{await command(a,'stay.check_out',{id:stays[a],reason:'Fictional same-day early departure'});});
  await phase('cash-shift-close',async a=>{await command(a,'shift.close',{id:shifts[a],counted:353100,handover:'Fictional front desk Finance review'});});
  const reviewer=(a:number)=>{const selected=16+(a%4);return selected===a?16+((a+1)%4):selected;};
  await Promise.all([16,17,18,19].map(async manager=>{for(let a=0;a<20;a++)if(reviewer(a)===manager)await command(manager,'shift.review',{id:shifts[a]},randomUUID(),a);}));
  const occupancy=await db.query("SELECT count(*)::int stays,bool_and(data_json::jsonb->>'state'='checked_out' AND data_json::jsonb->>'departure'=$3)::boolean completed FROM pms_records WHERE property_id=ANY($1::text[]) AND kind='stay' AND id=ANY($2::text[])",[config.properties.map(p=>p.id),stays,day(1)]);
  const balances=await db.query("SELECT count(*)::int accounts,sum((data_json::jsonb->>'due')::bigint)::text due,sum((data_json::jsonb->>'advance')::bigint)::text advance,sum((data_json::jsonb->>'deposit')::bigint)::text deposit FROM pms_records WHERE kind='guest_account' AND id=ANY($1::text[])",[accounts]);
  const cash=await db.query("SELECT count(*)::int shifts,sum((data_json::jsonb->>'cash')::bigint)::text cash,bool_and(data_json::jsonb->>'state'='reviewed' AND (data_json::jsonb->>'variance')::bigint=0)::boolean reviewed FROM pms_records WHERE kind='shift' AND id=ANY($1::text[])",[shifts]);
  const rooms=await db.query("SELECT count(*)::int rooms,bool_and(data_json::jsonb->>'cleanliness'='dirty' AND (data_json::jsonb->>'cleaningCycle')::int=1)::boolean dirty FROM pms_records WHERE kind='room' AND id=ANY($1::text[])",[[...assignedRooms,...movedRooms]]);
  const charges=await db.query("SELECT count(*)::int charges,sum((data_json::jsonb->>'total')::bigint)::text total FROM pms_records WHERE kind='entry' AND data_json::jsonb->>'kind'='charge' AND data_json::jsonb->>'guestAccountId'=ANY($1::text[])",[accounts]);
  const receipts=await db.query("SELECT count(*)::int receipts,sum(p.amount_minor)::text received,bool_and(p.status='settled')::boolean settled FROM pms_documents d JOIN finance_payments p ON p.id=d.finance_id AND p.company_id=$1 WHERE d.guest_account_id=ANY($2::text[]) AND d.kind='receipt'",[config.companyId,accounts]);
  const journals=await db.query("SELECT sum(l.debit_minor)::text debit,sum(l.credit_minor)::text credit FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=$1 AND j.status='posted'",[config.companyId]);
  const history=await db.query("SELECT count(*)::int records,sum(version)::text versions FROM pms_records WHERE property_id=ANY($1::text[]) AND kind IN ('reservation','stay','guest_account') AND id LIKE 'scale-history-%'",[config.properties.map(p=>p.id)]);
  operationalReconciliation={occupancy:occupancy.rows[0],balances:balances.rows[0],cash:cash.rows[0],rooms:rooms.rows[0],charges:charges.rows[0],receipts:receipts.rows[0],journals:journals.rows[0],history:history.rows[0]};
  if(occupancy.rows[0].stays!==20||!occupancy.rows[0].completed||balances.rows[0].accounts!==20||balances.rows[0].due!=='0'||balances.rows[0].advance!=='0'||balances.rows[0].deposit!=='0'||cash.rows[0].shifts!==20||cash.rows[0].cash!=='7062000'||!cash.rows[0].reviewed||rooms.rows[0].rooms!==40||!rooms.rows[0].dirty||charges.rows[0].charges!==20||charges.rows[0].total!=='7062000'||receipts.rows[0].receipts!==20||receipts.rows[0].received!=='7062000'||!receipts.rows[0].settled||!journals.rows[0].debit||journals.rows[0].debit!==journals.rows[0].credit||history.rows[0].records!==657000||history.rows[0].versions!=='657000')throw new Error('PMS_OPERATIONAL_PERFORMANCE_RECONCILIATION_FAILED');

  if(closingEnabled){
   stage='closing';
   console.log(JSON.stringify({event:'pms_closing_phase',phase:'guest-account-invoices',accounts:20}));
   await Promise.all(Array.from({length:20},(_,a)=>command(a,'guest_account.close',{id:accounts[a]})));
   const invoices=await db.query("SELECT count(*)::int invoices,sum(i.total_minor)::text total,sum(i.outstanding_minor)::text outstanding,bool_and(i.status='paid')::boolean paid FROM pms_records a JOIN finance_invoices i ON i.id=a.data_json::jsonb->>'invoiceId' AND i.company_id=$1 WHERE a.kind='guest_account' AND a.id=ANY($2::text[])",[config.companyId,accounts]);
   if(invoices.rows[0].invoices!==20||invoices.rows[0].total!=='7062000'||invoices.rows[0].outstanding!=='0'||!invoices.rows[0].paid)throw new Error('PMS_CLOSING_INVOICES_RECONCILIATION_FAILED');
   console.log(JSON.stringify({event:'pms_closing_phase',phase:'night-audit-start',properties:3}));
   const nights=await Promise.all(config.properties.map((_,p)=>command(16+p,'night.start',{},randomUUID(),p)));
   const deadline=Date.now()+30000;let completed=false;
   while(Date.now()<deadline){
    const jobs=await db.query("SELECT count(*)::int jobs,bool_and(state='completed')::boolean completed FROM pms_jobs WHERE property_id=ANY($1::text[]) AND id=ANY($2::text[])",[config.properties.map(p=>p.id),nights.map(n=>n.id)]);
    if(jobs.rows[0].jobs===3&&jobs.rows[0].completed){completed=true;break;}
    await new Promise(resolve=>setTimeout(resolve,250));
   }
   if(!completed)throw new Error('PMS_RELEASE_NIGHT_AUDIT_COMPLETION_REQUIRED');
   const days=await db.query("SELECT count(*)::int hotels,bool_and(business_date=$2 AND state='open')::boolean advanced FROM pms_properties WHERE id=ANY($1::text[])",[config.properties.map(p=>p.id),day(1)]);
   const nightsEvidence=await db.query("SELECT count(*)::int nights,sum((data_json::jsonb->'roomMetrics'->>'capacity')::int)::int capacity,sum((data_json::jsonb->'roomMetrics'->>'sold')::int)::int sold,bool_and(data_json::jsonb->>'state'='completed')::boolean completed FROM pms_records WHERE kind='night' AND id=ANY($1::text[])",[nights.map(n=>n.id)]);
   const afterJournals=await db.query("SELECT sum(l.debit_minor)::text debit,sum(l.credit_minor)::text credit FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=$1 AND j.status='posted'",[config.companyId]);
   const afterHistory=await db.query("SELECT count(*)::int records,sum(version)::text versions FROM pms_records WHERE property_id=ANY($1::text[]) AND kind IN ('reservation','stay','guest_account') AND id LIKE 'scale-history-%'",[config.properties.map(p=>p.id)]);
   if(days.rows[0].hotels!==3||!days.rows[0].advanced||nightsEvidence.rows[0].nights!==3||nightsEvidence.rows[0].capacity!==600||nightsEvidence.rows[0].sold!==20||!nightsEvidence.rows[0].completed||afterJournals.rows[0].debit!==journals.rows[0].debit||afterJournals.rows[0].credit!==journals.rows[0].credit||afterHistory.rows[0].records!==657000||afterHistory.rows[0].versions!=='657000')throw new Error('PMS_RELEASE_NIGHT_AUDIT_RECONCILIATION_FAILED');
   closingReconciliation={invoices:invoices.rows[0],days:days.rows[0],nights:nightsEvidence.rows[0],journals:afterJournals.rows[0],history:afterHistory.rows[0]};
  }
 }
 passed=true;
}finally{
 const current=JSON.parse(await readFile(join(directory,'state.json'),'utf8')),sameProcesses=alive(state.pid)&&alive(state.jobsPid)&&current.pid===state.pid&&current.jobsPid===state.jobsPid;
 const p95=(kind:Sample['kind'],selectedStage?:Sample['stage'])=>{const values=samples.filter(s=>s.kind===kind&&(!selectedStage||s.stage===selectedStage)).map(s=>s.elapsedMs).sort((a,b)=>a-b);return values.length?values[Math.ceil(values.length*.95)-1]:null;};
 const readP95=p95('read','core'),writeP95=p95('write','core'),financialP95=p95('write','cashier'),operationalP95=p95('write','operational'),operationalCount=samples.filter(s=>s.stage==='operational'&&s.kind==='write').length,financialCount=samples.filter(s=>s.stage==='cashier'&&s.kind==='write').length,unchanged=await runtimeSourceFingerprint(root)===sourceFingerprint&&await verificationSourceFingerprint(root,sourceFingerprint)===verificationFingerprint;
 const financialOperations=[...new Set(samples.filter(s=>s.stage==='cashier').map(s=>s.operation))].map(operation=>{
  const values=samples.filter(s=>s.stage==='cashier'&&s.operation===operation).map(s=>s.elapsedMs).sort((a,b)=>a-b);
  return {operation,count:values.length,p95Ms:values.length?values[Math.ceil(values.length*.95)-1]:null};
 });
 const operationalOperations=[...new Set(samples.filter(s=>s.stage==='operational').map(s=>s.operation))].map(operation=>{
  const values=samples.filter(s=>s.stage==='operational'&&s.operation===operation).map(s=>s.elapsedMs).sort((a,b)=>a-b);
  return {operation,count:values.length,p95Ms:values.length?values[Math.ceil(values.length*.95)-1]:null};
 });
 const closingCount=samples.filter(s=>s.stage==='closing'&&s.kind==='write').length,closingP95=p95('write','closing');
 const closingOperations=[...new Set(samples.filter(s=>s.stage==='closing').map(s=>s.operation))].map(operation=>{const values=samples.filter(s=>s.stage==='closing'&&s.operation===operation).map(s=>s.elapsedMs).sort((a,b)=>a-b);return {operation,count:values.length,p95Ms:values.length?values[Math.ceil(values.length*.95)-1]:null};});
 const result={run,sourceFingerprint,verificationFingerprint,release:state.release,scope:'Release HTTP: 3 × 200 rooms, 730-day synthetic closed stay/reservation/account metadata; 20 distinct normal staff accounts. Historic revenue is not invented.',applicationPoolMax:state.applicationPoolMax,controlPoolMax:state.controlPoolMax,jobsHoldingPoolMax:1,counts,accounts:20,rounds:10,durationMs:Date.now()-started,readCount:samples.filter(s=>s.kind==='read').length,writeCount:samples.filter(s=>s.kind==='write').length,readP95Ms:readP95,writeP95Ms:writeP95,coreReadCount:samples.filter(s=>s.stage==='core'&&s.kind==='read').length,coreWriteCount:samples.filter(s=>s.stage==='core'&&s.kind==='write').length,financialEnabled,financialWriteCount:financialCount,financialWriteP95Ms:financialP95,financialOperations,financialReconciliation,operationalEnabled,operationalWriteCount:operationalCount,operationalWriteP95Ms:operationalP95,operationalOperations,operationalReconciliation,closingEnabled,closingWriteCount:closingCount,closingWriteP95Ms:closingP95,closingOperations,closingReconciliation,failedSamples:samples.filter(s=>!s.valid).length,sameProcesses,unchanged,samplesSha256:createHash('sha256').update(await readFile(raw)).digest('hex'),passed:passed&&sameProcesses&&unchanged&&readP95!==null&&readP95<1000&&writeP95!==null&&writeP95<2000&&(!financialEnabled||(financialCount===200&&financialP95!==null&&financialP95<2000&&financialOperations.every(o=>o.p95Ms!==null&&o.p95Ms<2000)&&financialReconciliation!==null))&&(!operationalEnabled||(operationalCount===220&&operationalP95!==null&&operationalP95<2000&&operationalOperations.every(o=>o.p95Ms!==null&&o.p95Ms<2000)&&operationalReconciliation!==null))&&(!closingEnabled||(closingCount===23&&closingP95!==null&&closingP95<2000&&closingOperations.every(o=>o.p95Ms!==null&&o.p95Ms<2000)&&closingReconciliation!==null))};
 await writeFile(join(output,'result.json'),JSON.stringify(result,null,2)+'\n',{mode:0o600});
 await Promise.allSettled(contexts.map(c=>c.context.close()));await browser.close();await db.end();
 console.log(JSON.stringify({event:'pms_scale_result',passed:result.passed,readP95Ms:readP95,writeP95Ms:writeP95,coreReadCount:samples.filter(s=>s.stage==='core'&&s.kind==='read').length,coreWriteCount:samples.filter(s=>s.stage==='core'&&s.kind==='write').length,financialEnabled,financialWriteCount:financialCount,financialWriteP95Ms:financialP95,financialOperations,financialReconciliation,operationalEnabled,operationalWriteCount:operationalCount,operationalWriteP95Ms:operationalP95,operationalOperations,operationalReconciliation,closingEnabled,closingWriteCount:closingCount,closingWriteP95Ms:closingP95,closingOperations,closingReconciliation,failedSamples:result.failedSamples}));
 if(!result.passed)process.exitCode=1;
}
