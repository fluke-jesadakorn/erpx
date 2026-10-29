import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
vi.mock('next/headers',()=>({headers:vi.fn()}));
import {createNativeServiceInvoiceReader} from '../../../packages/feature-finance/src/adapters/server/native-service-invoice';
import type {NativeJournalPageContext} from '../../../packages/feature-finance/src/adapters/server/native-journal-page';
const config={privateBaseUrl:'http://127.0.0.1:43117',timeoutMs:250,holdings:[{origin:'https://acme.example.test',holdingSlug:'acme',holdingId:'h001',invoiceIds:['field-invoice-key']} ]};
const actor={userId:'reader',holdingId:'h001',holdingSlug:'acme',companyId:'company',branchId:null,departmentId:null,assignmentId:'grant',role:'finance',assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},modules:['finance'],iam:{permissions:['finance.read','module.finance.read']}} as NativeJournalPageContext;
const context={userId:'reader',holdingId:'h001',companyId:'company',branchId:null,departmentId:null,branchSlug:null,departmentSlug:null,assignmentId:'grant',assignmentVersion:3,revision:7,role:'finance',assignmentScopeKind:'holding',modules:['finance'],permissions:['finance.read','module.finance.read'],expiresAt:1800000000000};
const invoice={id:'field-invoice-key',company_id:'company',source_type:'field_service',source_id:'order',created_by:'author',kind:'receivable',invoice_number:'FS-order',invoice_date:'2026-10-29',status:'draft',version:1,debit_account_id:'ar',party_name:'Customer',currency:'USD',total_minor:200,outstanding_minor:200};
const data={invoice,allocations:[],lines:[],lineHistory:[],thaiDocuments:[],options:{charts:[{id:'chart',company_id:'company'}],accounts:[{id:'ar',company_id:'company',chart_id:'chart'}],periods:[],parties:[{id:'party',company_id:'company',address_json:{city:'Bangkok',latitude:13.75}}],invoices:[]},source:{releaseId:'field-billing-key',orderId:'order',commandKey:'key',actorId:'author',currency:'USD',amountUnits:'200',usdUnits:'200',fxRateId:null,usedLatestFallback:false}};
const incoming=new Headers({host:'acme.example.test',cookie:'opaque'});
const json=(v:unknown,status=200)=>new Response(JSON.stringify(v),{status,headers:{'content-type':'application/json'}});
describe('native Service invoice workspace',()=>{
 it('selects only listed IDs, sends the current Finance context and retains real owner rows',async()=>{
  const calls:URL[]=[];const read=createNativeServiceInvoiceReader(JSON.stringify(config),async(url,headers)=>{calls.push(url);expect(headers.get('cookie')).toBe('opaque');expect(headers.get('x-erpx-context-assignment-version')).toBe('3');return json({data,context,sourceOnly:true});});
  expect(await read(actor,incoming,'acme','other')).toBeUndefined();expect(calls).toHaveLength(0);
  const result=await read(actor,incoming,'acme','field-invoice-key');expect(calls[0]?.pathname).toBe('/api/v1/holdings/acme/finance/invoices/field-invoice-key/workspace');expect(result?.source.releaseId).toBe('field-billing-key');expect(result?.options.parties[0]?.address_json).toBe('{"city":"Bangkok","latitude":13.75}');
 });
 it('fails closed on native denial, mismatched identity, original context or Money evidence',async()=>{
  for(const [reply,status] of [[null,403],[{data:{...data,invoice:{...invoice,id:'other'}},context,sourceOnly:true},200],[{data:{...data,source:{...data.source,amountUnits:'201'}},context,sourceOnly:true},200],[{data,context:{...context,userId:'other'},sourceOnly:true},200]] as const){
   const read=createNativeServiceInvoiceReader(JSON.stringify(config),async()=>json(reply,status));await expect(read(actor,incoming,'acme','field-invoice-key')).rejects.toBeDefined();
  }
  const read=createNativeServiceInvoiceReader(JSON.stringify(config),async()=>json({data,context,sourceOnly:true}));await expect(read(actor,new Headers({host:'foreign.example.test'}),'acme','field-invoice-key')).rejects.toMatchObject({code:'NATIVE_FINANCE_AUTHORITY_INVALID'});
 });
 it('rejects duplicate selections and invalid private endpoints without a fallback',()=>{
  expect(()=>createNativeServiceInvoiceReader(JSON.stringify({...config,holdings:[{...config.holdings[0],invoiceIds:['field-invoice-key','field-invoice-key']}]}))).toThrow();
  expect(()=>createNativeServiceInvoiceReader(JSON.stringify({...config,privateBaseUrl:'http://127.0.0.1:3100'}))).toThrow();
 });
});
