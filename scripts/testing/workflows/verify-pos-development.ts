/** Verify browser-created fictional bills against the development business services. */
import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {dirname,join,resolve} from 'node:path';
import {mock} from 'bun:test';
import {assertHospitalityDevelopmentTarget} from "@erpx/feature-shared/lib/hospitality-seed-target";
import {businessRoles} from "@erpx/feature-identity_access/lib/business-roles";
import {posDemoCompany as company} from "../../../fixtures/pos/catalog";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {PostgresDatabase} from "@erpx/server-runtime/adapters/runtime/postgres";
mock.module('next/navigation',()=>({forbidden:()=>{throw new Error('CLI_PAGE_FORBIDDEN');},redirect:()=>{throw new Error('CLI_REDIRECT_FORBIDDEN');}}));
const manifestPath=process.env.ERPX_DATABASE_MANIFEST;if(!manifestPath)throw new Error('DEVELOPMENT_MANIFEST_REQUIRED');
const app=JSON.parse(await readFile(manifestPath,'utf8')),jobsPath=join(dirname(manifestPath),'databases-jobs.json'),jobs=JSON.parse(await readFile(jobsPath,'utf8'));
assertHospitalityDevelopmentTarget(app,process.env.ERPX_ENVIRONMENT);assertHospitalityDevelopmentTarget(jobs,process.env.ERPX_ENVIRONMENT);
const flag=(name:string)=>process.argv.find(arg=>arg.startsWith(`--${name}=`))?.slice(name.length+3);
const numbers=[flag('room-sale'),flag('retail-sale')];if(numbers.some(number=>!number))throw new Error('PASS_BROWSER_ROOM_AND_RETAIL_BILL_NUMBERS');
process.env.ERPX_DATABASE_MANIFEST=jobsPath;process.env.ERPX_PROCESS_ROLE='jobs';
const {closeRuntime}=await import("@erpx/server-runtime/adapters/runtime/env");
const {posCommand}=await import("@erpx/server-pos/public/pos-core");
const {one,all}=await import("@erpx/server-runtime/public/runtime/sql");
const {posDayReport}=await import("@erpx/server-pos/public/pos-daily");
const db=new PostgresDatabase({connectionString:jobs.databases.H001_DB,max:2});
const actor=(label:'manager'|'finance')=>({userId:`pos-thb-v1-${label}`,assignmentId:`pos-thb-v1-${label}-access`,role:label==='manager'?'pos_manager':'finance_approver',companyId:company,branchId:null,departmentId:null,holdingId:'h001',holdingSlug:'acme',holdingName:'ACME',name:label,email:`${label}@pos-thb-v1.example`,assignmentScope:'holding',inherited:false,modules:[...businessRoles[label==='manager'?'pos_manager':'finance_approver']],assignments:[],workspaces:[],scope:{kind:'holding',holdingId:'h001',holdingSlug:'acme',holdingName:'ACME',companyId:company}} as WorkContext);
try{
 const sales=[];
 for(const number of numbers){const sale=await one(db,'SELECT id,number,company_id,branch_id,business_date,shift_id,state,invoice_id,subtotal_minor,tax_minor,total_minor FROM pos_sales WHERE company_id=? AND number=?',company,number);if(!sale||sale.state!=='completed')throw new Error(`BROWSER_BILL_NOT_COMPLETED:${number}`);sales.push(sale);}
 const manager=actor('manager'),finance=actor('finance');
 for(const sale of sales){
  const shift=(await one(db,'SELECT * FROM pos_shifts WHERE id=? AND company_id=?',sale.shift_id,company))!;
  if(process.argv.includes('--review-shifts')&&shift.state==='closed')await posCommand(db,manager,'shift.review',{id:shift.id,expectedVersion:shift.version,reason:'Independent review of fictional browser acceptance drawer count'},`pos-browser-review:${shift.id}:v${shift.version}`);
  if(process.argv.includes('--confirm-days')){
   const close=await one(db,'SELECT * FROM pos_day_closes WHERE company_id=? AND branch_id=? AND business_date=?',company,sale.branch_id,sale.business_date);
   if(!close)throw new Error('PREPARE_DAY_IN_BROWSER_FIRST');
   if(close.state==='prepared'){
    const before=await one(db,'SELECT COUNT(*) n FROM finance_journals WHERE company_id=?',company);
    await posCommand(db,finance,'day.confirm',{id:close.id,expectedVersion:close.version},`pos-browser-confirm:${close.id}:v${close.version}`);
    const after=await one(db,'SELECT COUNT(*) n FROM finance_journals WHERE company_id=?',company);
    if(before?.n!==after?.n)throw new Error('DAY_CONFIRM_REPOSTED_A_JOURNAL');
   }
  }
 }
 const bills=[];
 for(const [index,sale] of sales.entries()){
  const receipt=await one(db,'SELECT snapshot_json FROM pos_sale_documents WHERE sale_id=?',sale.id);
  const snapshot=typeof receipt?.snapshot_json==='string'?JSON.parse(receipt.snapshot_json):receipt?.snapshot_json;
  if(!snapshot)throw new Error('RECEIPT_SNAPSHOT_MISSING');
  const payments=await all(db,'SELECT id,method,state,amount_minor,tendered_minor,tendered_minor-amount_minor change_minor,finance_payment_id FROM pos_payments WHERE sale_id=?',sale.id);
  const invoices=await all(db,"SELECT id,status,total_minor,journal_id FROM finance_invoices WHERE company_id=? AND source_type='pos' AND source_id=?",company,sale.id);
  const postings=await all(db,'SELECT p.source_key,p.journal_id,j.status FROM pos_postings p JOIN finance_journals j ON j.id=p.journal_id WHERE p.company_id=? AND (j.source_id=? OR j.source_id IN (SELECT fulfillment_id FROM pos_lines WHERE sale_id=?))',company,sale.id,sale.id);
  const stock=await all(db,'SELECT m.product_id,m.kind,m.quantity_milli,m.value_minor FROM pos_stock_moves m WHERE m.fulfillment_id IN (SELECT fulfillment_id FROM pos_lines WHERE sale_id=?) ORDER BY m.product_id',sale.id);
  const room=await one(db,'SELECT property_id,record_id FROM pms_sources WHERE source_key=?',`pos:${sale.id}:${sale.id}`);
  if(index===0&&(payments.length!==0||invoices.length!==0||!room||postings.some(p=>String(p.source_key).startsWith('sale:')||String(p.source_key).startsWith('receipt:'))))throw new Error('ROOM_REVENUE_OWNERSHIP_FAILED');
  if(index===1&&(payments.length!==1||payments[0].state!=='succeeded'||payments[0].method!=='cash'||invoices.length!==1||stock.length!==1||stock[0].quantity_milli!==-1000))throw new Error('RETAIL_SOURCE_COUNTS_FAILED');
  const shift=await one(db,'SELECT id,state,opened_by,closed_by,reviewed_by,expected_minor,counted_minor,variance_minor,denominations_json FROM pos_shifts WHERE id=?',sale.shift_id);
  const report=await posDayReport(db,manager,String(sale.branch_id),String(sale.business_date));
  const close=await one(db,'SELECT id,state,prepared_by,confirmed_by,source_digest FROM pos_day_closes WHERE company_id=? AND branch_id=? AND business_date=?',company,sale.branch_id,sale.business_date);
  bills.push({sale,payments,invoices,postings,stock,roomCharge:room,receipt:snapshot,shift,day:{close,totals:report.totals,blockers:report.blockers}});
 }
 const proof={schema:1,developmentOnly:true,company,checkedAt:new Date().toISOString(),independentReviewThroughBusinessServices:true,bills};
 const directory=resolve('.scratch/dev/pos-browser-acceptance');await mkdir(directory,{recursive:true,mode:0o700});await writeFile(join(directory,'verification.json'),JSON.stringify(proof,null,2),{mode:0o600});
 console.log(JSON.stringify(proof,null,2));
}finally{await db.close();await closeRuntime();}
