import {createNativeFieldFinancialWorkReader} from '../../../packages/server-finance/src/adapters/native-field-work';
import {createHash} from 'node:crypto';
import {createNativeServiceCustomerReceiptReader} from '../../../packages/feature-finance/src/adapters/server/native-service-customer-receipt';
import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
vi.mock('next/headers',()=>({headers:vi.fn()}));
import {createNativeServicePaymentCommands} from '../../../packages/feature-finance/src/adapters/server/native-service-payment-command';
import {createNativeServicePaymentReader} from '../../../packages/feature-finance/src/adapters/server/native-service-payment';
import {createNativeServiceInvoiceReader} from '../../../packages/feature-finance/src/adapters/server/native-service-invoice';
import {createNativeFinanceSetupWorkspaceReader,nativeFinanceSetupWorkspace} from '../../../packages/feature-finance/src/adapters/server/native-finance-setup';
import {NativeFinanceReadError} from '../../../packages/feature-finance/src/adapters/server/native-journal-detail';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {NativeJournalPageContext} from '../../../packages/feature-finance/src/adapters/server/native-journal-page';
const config={privateBaseUrl:'http://127.0.0.1:43117',timeoutMs:250,holdings:[{origin:'https://acme.example.test',holdingSlug:'acme',holdingId:'h001'} ]};
const actor={userId:'reader',holdingId:'h001',holdingSlug:'acme',companyId:'company',branchId:null,departmentId:null,assignmentId:'grant',role:'finance',assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},modules:['finance'],iam:{permissions:['finance.read','module.finance.read']}} as NativeJournalPageContext;
const context={userId:'reader',holdingId:'h001',companyId:'company',branchId:null,departmentId:null,branchSlug:null,departmentSlug:null,assignmentId:'grant',assignmentVersion:3,revision:7,role:'finance',assignmentScopeKind:'holding',modules:['finance'],permissions:['finance.read','module.finance.read'],expiresAt:1800000000000};
const invoice={id:'field-invoice-key',company_id:'company',source_type:'field_service',source_id:'order',created_by:'author',kind:'receivable',invoice_number:'FS-order',invoice_date:'2026-10-29',status:'draft',version:1,debit_account_id:'ar',party_name:'Customer',currency:'USD',total_minor:200,outstanding_minor:200};
const data={customerReceipts:[],postingPreview:{state:'ready',posted:false,accounts:[],lines:[{id:'ar',debit:200,credit:0},{id:'revenue',debit:0,credit:200}],debit:200,credit:200},invoice,allocations:[],lines:[],lineHistory:[],thaiDocuments:[],options:{charts:[{id:'chart',company_id:'company'}],accounts:[{id:'ar',company_id:'company',chart_id:'chart'}],periods:[],parties:[{id:'party',company_id:'company',address_json:{city:'Bangkok',latitude:13.75}}],invoices:[]},source:{releaseId:'field-billing-key',orderId:'order',commandKey:'key',actorId:'author',currency:'USD',amountUnits:'200',usdUnits:'200',fxRateId:null,usedLatestFallback:false}};
const purchaseControls=Object.fromEntries(['finance.accounting','finance.banking','finance.close','finance.advanced','finance.receivables','finance.tax','finance.service-invoice'].map(cap=>[cap,{newWork:false,settleExisting:cap==='finance.service-invoice',history:cap==='finance.service-invoice',revision:'current-purchase'}]));
const nativeFlow={kind:'invoice',recordId:invoice.id,recordVersion:1,version:0,defaultsRevision:0,state:'draft',steps:[{key:'draft',action:'Review',role:'finance_approver',allowedRoles:['finance_approver','holding_owner'],teamName:'Finance',primary:null,candidates:[],status:'current',blocker:'Select a primary'},{key:'posted',action:'Review',role:'finance_approver',allowedRoles:['finance_approver','holding_owner'],teamName:'Finance',primary:null,candidates:[],status:'upcoming',blocker:'Select a primary'}],current:{key:'draft',action:'Review',role:'finance_approver',allowedRoles:['finance_approver','holding_owner'],teamName:'Finance',primary:null,candidates:[],status:'current',blocker:'Select a primary'},next:null,canAct:false,canManage:true,blocker:'Select a primary',history:[]};
const invoicePresentation={noCharge:null,flow:nativeFlow,purchaseControls,sourcePurchase:{capability:'finance.service-invoice',settleExisting:true,history:true,revision:'current-purchase'},canPost:true};
const incoming=new Headers({host:'acme.example.test',cookie:'opaque'});
const json=(v:unknown,status=200)=>new Response(JSON.stringify(v),{status,headers:{'content-type':'application/json'}});
describe('native Service invoice workspace',()=>{
 it('observes actual source ownership before the current Finance workspace read',async()=>{
  const calls:URL[]=[];const read=createNativeServiceInvoiceReader(JSON.stringify(config),async(url,headers)=>{calls.push(url);expect(headers.get('cookie')).toBe('opaque');expect(headers.get('x-erpx-context-assignment-version')).toBe('3');return json(url.pathname.endsWith('/post-runtime')?{data:{id:url.pathname.split('/').at(-2),runtime:url.pathname.includes('/other/')?'legacy':'rust'}}:{data,context,sourceOnly:true,...invoicePresentation});});
  expect(await read(actor,incoming,'acme','other')).toBeUndefined();expect(calls).toHaveLength(1);expect(calls[0]?.pathname).toBe('/api/v1/holdings/acme/finance/invoices/other/post-runtime');
  const result=await read(actor,incoming,'acme','field-invoice-key');expect(calls[1]?.pathname).toBe('/api/v1/holdings/acme/finance/invoices/field-invoice-key/post-runtime');expect(calls[2]?.pathname).toBe('/api/v1/holdings/acme/finance/invoices/field-invoice-key/workspace');expect(result?.source.releaseId).toBe('field-billing-key');expect(result?.options.parties[0]?.address_json).toBe('{"city":"Bangkok","latitude":13.75}');
 });
 it('fails closed on native denial, mismatched identity, original context or Money evidence',async()=>{
  for(const [reply,status] of [[null,403],[{data:{...data,postingPreview:{...data.postingPreview,debit:Number.MAX_SAFE_INTEGER+1}},context,sourceOnly:true,...invoicePresentation},200],[{data:{...data,invoice:{...invoice,id:'other'}},context,sourceOnly:true,...invoicePresentation},200],[{data:{...data,source:{...data.source,amountUnits:'201'}},context,sourceOnly:true,...invoicePresentation},200],[{data,context:{...context,userId:'other'},sourceOnly:true,...invoicePresentation},200]] as const){
   const read=createNativeServiceInvoiceReader(JSON.stringify(config),async u=>u.pathname.endsWith('/post-runtime')?json({data:{id:'field-invoice-key',runtime:'rust'}}):json(reply,status));await expect(read(actor,incoming,'acme','field-invoice-key')).rejects.toBeDefined();
  }
  const read=createNativeServiceInvoiceReader(JSON.stringify(config),async()=>json({data,context,sourceOnly:true,...invoicePresentation}));await expect(read(actor,new Headers({host:'foreign.example.test'}),'acme','field-invoice-key')).rejects.toMatchObject({code:'NATIVE_FINANCE_AUTHORITY_INVALID'});
 });
 it('binds no-charge controls to the exact zero invoice, version and terminal state',async()=>{
  const noCharge={invoiceId:invoice.id,version:1,contextKey:'a'.repeat(64),canConfirm:true,confirmed:false};
  const zero={...data,invoice:{...invoice,total_minor:0,outstanding_minor:0},source:{...data.source,amountUnits:'0',usdUnits:'0'},postingPreview:{state:'ready',posted:false,accounts:[],lines:[],debit:0,credit:0}};
  const read=(controls:unknown,source=zero)=>createNativeServiceInvoiceReader(JSON.stringify(config),async url=>json(url.pathname.endsWith('/post-runtime')?{data:{id:invoice.id,runtime:'rust'}}:{data:source,context,sourceOnly:true,...invoicePresentation,noCharge:controls}))(actor,incoming,'acme',invoice.id);
  expect((await read(noCharge))?.noCharge).toEqual(noCharge);
  for(const bad of [{...noCharge,invoiceId:'other'},{...noCharge,version:2},{...noCharge,contextKey:'bad'},{...noCharge,confirmed:true},{...noCharge,canConfirm:'yes'},undefined])await expect(read(bad)).rejects.toMatchObject({code:'NATIVE_FINANCE_RESPONSE_INVALID'});
  await expect(read(noCharge,{...zero,invoice:{...invoice}})).rejects.toMatchObject({code:'NATIVE_FINANCE_RESPONSE_INVALID'});
 });
 it('never selects legacy from a failed or mismatched source observation',async()=>{
  for(const reply of [{data:{id:'another',runtime:'legacy'}},{data:{id:'field-invoice-key',runtime:'unknown'}}]){
   const transport=vi.fn(async()=>json(reply));await expect(createNativeServiceInvoiceReader(JSON.stringify(config),transport)(actor,incoming,'acme','field-invoice-key')).rejects.toMatchObject({code:'NATIVE_FINANCE_RESPONSE_INVALID'});expect(transport).toHaveBeenCalledTimes(1);
  }
  for(const status of [403,409,503]){const transport=vi.fn(async()=>json({error:{code:'DENIED'}},status));await expect(createNativeServiceInvoiceReader(JSON.stringify(config),transport)(actor,incoming,'acme','field-invoice-key')).rejects.toMatchObject({status});expect(transport).toHaveBeenCalledTimes(1);}
 });
 it('rejects duplicate Holding configuration and invalid private endpoints without a fallback',()=>{
  expect(()=>createNativeServiceInvoiceReader(JSON.stringify({...config,holdings:[config.holdings[0],config.holdings[0]]}))).toThrow();
  expect(()=>createNativeServiceInvoiceReader(JSON.stringify({...config,privateBaseUrl:'http://127.0.0.1:3100'}))).toThrow();
 });
});


describe('native Service payment workspace',()=>{
 const paymentConfig={...config,holdings:config.holdings.map(({origin,holdingSlug,holdingId})=>({origin,holdingSlug,holdingId}))};
 const payment={id:'payment-key',company_id:'company',kind:'receipt',status:'released',version:3,amount_minor:10000,outstanding_minor:10000,currency:'THB',created_by:'creator',settlement_invoice_id:'field-invoice-key'};
 const paymentData={payment,allocations:[],reversal:null,periods:[],invoices:[],bankTransactions:[],fxAccounts:[],sourcePurchase:{capability:'finance.service-invoice',settleExisting:true,history:true,revision:'actual-purchase'}};
 it('uses the server source verdict before reading a native payment with the captured context',async()=>{
  const calls:URL[]=[];
  const read=createNativeServicePaymentReader(JSON.stringify(paymentConfig),async(url,h)=>{calls.push(url);expect(h.get('cookie')).toBe('opaque');expect(h.get('x-erpx-context-revision')).toBe('7');return json(url.pathname.endsWith('/runtime')?{data:{id:'payment-key',runtime:'rust'}}:{data:paymentData,context,filesMode:'enabled',sourceOnly:true,purchaseControls});});
  expect((await read(actor,incoming,'acme','payment-key'))?.nativeFilesMode).toBe('enabled');expect(calls.map(u=>u.pathname)).toEqual(['/api/v1/holdings/acme/finance/payments/payment-key/runtime','/api/v1/holdings/acme/finance/payments/payment-key/workspace']);
 });
 it('returns legacy only for a successful matching persisted-source verdict',async()=>{
  const transport=vi.fn(async()=>json({data:{id:'payment-key',runtime:'legacy'}}));
  expect(await createNativeServicePaymentReader(JSON.stringify(paymentConfig),transport)(actor,incoming,'acme','payment-key')).toBeUndefined();expect(transport).toHaveBeenCalledTimes(1);
  for(const reply of [{data:{id:'other',runtime:'legacy'}},{data:{id:'payment-key',runtime:'unknown'}}]){
   await expect(createNativeServicePaymentReader(JSON.stringify(paymentConfig),async()=>json(reply))(actor,incoming,'acme','payment-key')).rejects.toMatchObject({code:'NATIVE_FINANCE_RESPONSE_INVALID'});
  }
 });
 it('never falls back after a native verdict when a transport or owner read fails',async()=>{
  for(const status of [403,409,503]){
   await expect(createNativeServicePaymentReader(JSON.stringify(paymentConfig),async u=>u.pathname.endsWith('/runtime')?json({data:{id:'payment-key',runtime:'rust'}}):json({error:{code:'DENIED'}},status))(actor,incoming,'acme','payment-key')).rejects.toMatchObject({code:'NATIVE_FINANCE_UNAVAILABLE',status});
  }
  await expect(createNativeServicePaymentReader(JSON.stringify(paymentConfig),async()=>{throw Error('socket closed')})(actor,incoming,'acme','payment-key')).rejects.toMatchObject({code:'NATIVE_FINANCE_UNAVAILABLE'});
 });
 it('rejects changed identity, unrelated source allocations and unsafe Money',async()=>{
  for(const reply of [{data:paymentData,context:{...context,userId:'other'}},{data:{...paymentData,allocations:[{invoice_id:'unrelated'}]},context},{data:{...paymentData,payment:{...payment,amount_minor:Number.MAX_SAFE_INTEGER+1}},context}]){
   const read=createNativeServicePaymentReader(JSON.stringify(paymentConfig),async u=>json(u.pathname.endsWith('/runtime')?{data:{id:'payment-key',runtime:'rust'}}:{filesMode:'enabled',sourceOnly:true,purchaseControls,...reply}));await expect(read(actor,incoming,'acme','payment-key')).rejects.toBeDefined();
  }
 });
});


describe('native Service payment command transport',()=>{
 const rawConfig=JSON.stringify({...config,holdings:config.holdings.map(({origin,holdingSlug,holdingId})=>({origin,holdingSlug,holdingId}))});
 const command=(body:string)=>new Request('https://acme.example.test/api/payment',{method:'POST',headers:{host:'acme.example.test',origin:'https://acme.example.test',cookie:'opaque','idempotency-key':'original-key','x-erpx-context-company':'company','x-erpx-context-assignment':'grant','x-erpx-context-revision':'7','x-erpx-context-assignment-version':'3'},body});
 it('forwards original ordered raw bytes and key only after a matching Rust verdict',async()=>{
  const raw=String.raw`{"proof":"\ud800","expectedVersion":1,"ignored":true}`;
  const calls:{url:URL;body:Uint8Array;method:unknown}[]=[];
  const proxy=createNativeServicePaymentCommands(rawConfig,async(url,h,b,_s,method)=>{calls.push({url,body:b,method});expect(h.get('cookie')).toBe('opaque');if(method==='GET')return json({data:{id:'payment-key',runtime:'rust'}});expect(h.get('idempotency-key')).toBe('original-key');expect(new TextDecoder().decode(b)).toBe(raw);return json({data:{id:'payment-key',status:'approved',version:2}});});
  expect((await proxy(command(raw),actor,'acme',['payments','payment-key','approve'],JSON.parse(raw)))?.status).toBe(200);expect(calls.map(c=>[c.method,c.url.pathname])).toEqual([['GET','/api/v1/holdings/acme/finance/payments/payment-key/runtime'],['POST','/api/v1/holdings/acme/finance/payments/payment-key/approve']]);
 });
 it('retains legacy bodies only for the explicit non-Field verdict and refuses native failures',async()=>{
  const request=command('{}'),legacy=vi.fn(async()=>json({data:{id:'payment-key',runtime:'legacy'}}));expect(await createNativeServicePaymentCommands(rawConfig,legacy)(request,actor,'acme',['payments','payment-key','confirm'],{})).toBeUndefined();expect(request.bodyUsed).toBe(false);expect(legacy).toHaveBeenCalledTimes(1);
  const denied=await createNativeServicePaymentCommands(rawConfig,async()=>json({error:{code:'PAYMENT_OWNER_UNKNOWN',message:'Source unavailable'}},503))(command('{}'),actor,'acme',['payments','payment-key','confirm'],{});expect(denied?.status).toBe(503);expect((await denied?.json()).error.code).toBe('PAYMENT_OWNER_UNKNOWN');
  const wrong=await createNativeServicePaymentCommands(rawConfig,async()=>json({data:{id:'other',runtime:'legacy'}}))(command('{}'),actor,'acme',['payments','payment-key','confirm'],{});expect(wrong?.status).toBe(502);
 });
 it('routes source invoice creation and exact receipt bank evidence with correlated results',async()=>{
  for(const [parts,input,id,path,data] of [
   [['payments'],{settlementInvoiceId:'field-invoice-key'},'field-invoice-key','/finance/payments',{id:'pay-9e0fb673-6d91-43a8-811e-65c62228e9ea',status:'draft',version:1,settlementInvoiceId:'field-invoice-key'}],
   [['bank','statements','import'],{settlementPaymentId:'payment-key'},'payment-key','/finance/bank/statements/import',{id:'statement-original-key',settlementPaymentId:'payment-key',imported:1}],
  ] as const){
   const proxy=createNativeServicePaymentCommands(rawConfig,async(url,_h,_b,_s,method)=>method==='GET'?json({data:{id,runtime:'rust'}}):(expect(url.pathname.endsWith(path)).toBe(true),json({data},201)));
   expect((await proxy(command(JSON.stringify(input)),actor,'acme',parts,input))?.status).toBe(201);
  }
 });
 it('correlates the issued receipt to its source without deriving its ID from the command key',async()=>{
  const rawConfig=JSON.stringify(config),input={settlementInvoiceId:'field-invoice-key'},issued={id:'pay-9e0fb673-6d91-43a8-811e-65c62228e9ea',status:'draft',version:1,settlementInvoiceId:input.settlementInvoiceId};
  for(const data of [issued,{...issued,id:''},{...issued,id:'x'.repeat(181)},{...issued,id:'../receipt'},{...issued,settlementInvoiceId:'foreign-invoice'},{...issued,status:'approved'},{...issued,version:2}]){
   const transport=vi.fn(async(_url:URL,_headers:Headers,_body:Uint8Array,_signal:AbortSignal,method:'GET'|'POST'='POST')=>method==='GET'?json({data:{id:input.settlementInvoiceId,runtime:'rust'}}):json({data},201));
   const response=await createNativeServicePaymentCommands(rawConfig,transport)(command(JSON.stringify(input)),actor,'acme',['payments'],input);
   expect(response?.status).toBe(data===issued?201:502);if(data===issued)expect((await response!.json()).data).toEqual(issued);expect(transport).toHaveBeenCalledTimes(2);
  }
 });
 it('does no work on changed context and never routes unsupported Field actions to legacy',async()=>{
  const transport=vi.fn(async()=>json({data:{id:'payment-key',runtime:'rust'}})),proxy=createNativeServicePaymentCommands(rawConfig,transport),stale=command('{}');stale.headers.set('x-erpx-context-revision','8');expect((await proxy(stale,actor,'acme',['payments','payment-key','approve'],{}))?.status).toBe(409);expect(transport).not.toHaveBeenCalled();
  expect((await proxy(command('{}'),actor,'acme',['payments','payment-key','reverse'],{}))?.status).toBe(503);expect(transport).toHaveBeenCalledTimes(1);
 });
 it('retains identical request bytes/key across a lost native reply without an automatic second write',async()=>{
  const writes:{key:string|null;body:string}[]=[];
  const proxy=createNativeServicePaymentCommands(rawConfig,async(_u,h,b,_s,method)=>{if(method==='GET')return json({data:{id:'payment-key',runtime:'rust'}});writes.push({key:h.get('idempotency-key'),body:new TextDecoder().decode(b)});if(writes.length===1)throw Error('reply lost after commit');return json({data:{id:'payment-key',status:'settled',version:4}});});
  const raw='{"expectedVersion":3,"proof":"Original proof"}';
  expect((await proxy(command(raw),actor,'acme',['payments','payment-key','confirm'],JSON.parse(raw)))?.status).toBe(503);expect(writes).toHaveLength(1);expect((await proxy(command(raw),actor,'acme',['payments','payment-key','confirm'],JSON.parse(raw)))?.status).toBe(200);expect(writes).toEqual([{key:'original-key',body:raw},{key:'original-key',body:raw}]);
 });
});

describe('native Field immutable receipt readback',()=>{
 const id='customer-receipt-payment-key',html='<!doctype html><title>Original issued receipt</title>',sha=createHash('sha256').update(html).digest('hex');
 const snapshot={schema:'finance.customer-receipt.v1',documentNumber:'RC-1',companyName:'Company',customerName:'Customer',customerCode:'C-1',date:'2026-11-01',currency:'THB',amountMinor:'200',paymentId:'payment-key',paymentVersion:4,bankReference:'RCV-1',journalId:'journal-key',allocations:[{invoiceId:invoice.id,invoiceNumber:'FS-1',amountMinor:'200'}]};
 const receipt={id,company_id:'company',payment_id:'payment-key',document_number:'RC-1',receipt_date:'2026-11-01',currency:'THB',amount_minor:200,payment_version:4,payment_status:'settled'};
 const copy=()=>({data:{receipt,snapshot,status:'issued',sha256:sha},export:{content:html,sha256:sha,document_number:'RC-1'},context});
 it('reads only the persisted native owner and retains exact HTML/hash and render identity',async()=>{
  const paths:string[]=[];
  const read=createNativeServiceCustomerReceiptReader(JSON.stringify(config),async(url,h)=>{paths.push(url.pathname);expect(h.get('cookie')).toBe('opaque');return json(url.pathname.endsWith('/runtime')?{data:{id,runtime:'rust'}}:copy());});
  const result=await read(actor,new Headers({host:'acme.example.test',cookie:'opaque'}),'acme',id);
  expect(result?.data.snapshot).toEqual(snapshot);expect(result?.export.content).toBe(html);expect(result?.data.sha256).toBe(sha);
  expect(paths).toEqual(['/api/v1/holdings/acme/finance/sales/receipts/'+id+'/runtime','/api/v1/holdings/acme/finance/sales/receipts/'+id+'/workspace']);
 });
 it('uses legacy only after a successful matching persisted-source observation',async()=>{
  const transport=vi.fn(async()=>json({data:{id,runtime:'legacy'}}));
  expect(await createNativeServiceCustomerReceiptReader(JSON.stringify(config),transport)(actor,new Headers({host:'acme.example.test'}),'acme',id)).toBeUndefined();
  expect(transport).toHaveBeenCalledTimes(1);
 });
 it.each(['hash','company','snapshot','unsafeMoney','identity','expired','htmlLeak'])('rejects corrupt/stale %s without a legacy fallback',async(change)=>{
  const value=structuredClone(copy());
  if(change==='hash')value.export.content+='changed';
  if(change==='company')value.data.receipt.company_id='other';
  if(change==='snapshot')value.data.snapshot.paymentId='other';
  if(change==='unsafeMoney')value.data.snapshot.amountMinor='9007199254740992';
  if(change==='identity')value.context.userId='previous-account';
  if(change==='expired')value.context.expiresAt=1;
  if(change==='htmlLeak')Object.assign(value.data.receipt,{export_html:html});
  const read=createNativeServiceCustomerReceiptReader(JSON.stringify(config),async url=>json(url.pathname.endsWith('/runtime')?{data:{id,runtime:'rust'}}:value));
  await expect(read(actor,new Headers({host:'acme.example.test'}),'acme',id)).rejects.toThrow();
 });
 it('closes on unavailable owner observation rather than selecting legacy',async()=>{
  const read=createNativeServiceCustomerReceiptReader(JSON.stringify(config),async()=>json({error:{code:'UNAVAILABLE'}},503));
  await expect(read(actor,new Headers({host:'acme.example.test'}),'acme',id)).rejects.toMatchObject({status:503});
 });
});

it('forwards native invoice assignment bytes/key after persisted source observation despite a client legacy label',async()=>{
 const raw='{"stepKey":"draft","userId":"reviewer","expectedVersion":0,"recordVersion":1,"reason":"Independent owner","source":"legacy"}';
 const paths:string[]=[];
 const send=createNativeServicePaymentCommands(JSON.stringify(config),async(url,h,bytes,_signal,method)=>{
  paths.push(url.pathname);
  if(method==='GET')return json({data:{id:invoice.id,runtime:'rust'}});
  expect(new TextDecoder().decode(bytes)).toBe(raw);expect(h.get('idempotency-key')).toBe('native-flow-key');
  return json({data:{id:invoice.id,version:2}});
 });
 const request=new Request('https://acme.example.test/api/v1/holdings/acme/flows/invoice/'+invoice.id,{method:'POST',headers:{host:'acme.example.test',origin:'https://acme.example.test',cookie:'opaque','idempotency-key':'native-flow-key','x-erpx-context-company':'company','x-erpx-context-assignment':'grant','x-erpx-context-revision':'7','x-erpx-context-assignment-version':'3'},body:raw});
 expect((await send(request,actor,'acme',['invoices',invoice.id,'flow'],JSON.parse(raw)))?.status).toBe(200);
 expect(paths).toEqual(['/api/v1/holdings/acme/finance/invoices/'+invoice.id+'/post-runtime','/api/v1/holdings/acme/finance/invoices/'+invoice.id+'/flow']);
});

describe('native Field-parts journal responsibility transport',()=>{
 const staff={userId:'reader',name:'Current reviewer',teamId:null,teamName:'Finance'};
 const step={key:'draft',action:'Review Debit / Credit and post GL',role:'finance_approver',allowedRoles:['finance_approver','holding_owner'],teamName:'Finance',primary:staff,candidates:[staff],status:'current',blocker:null};
 const journalFlow={kind:'journal',recordId:'cost',recordVersion:1,version:0,defaultsRevision:0,state:'draft',steps:[step],current:step,next:null,canAct:true,canManage:true,blocker:null,history:[]};
 const workspace={
  journal:{id:'cost',company_id:'company',chart_id:'chart',journal_number:'FC-1',entry_date:'2026-01-10',version:1,status:'draft',source_type:'field_service_parts',created_by_name:'Original technician',posted_by_name:'Unlinked account'},
  lines:[{id:'cost-debit',journal_id:'cost',account_id:'expense',code:'510',name:'Spare cost',ordinal:0,debit_minor:600,credit_minor:0},{id:'cost-credit',journal_id:'cost',account_id:'inventory',code:'110',name:'Inventory',ordinal:1,debit_minor:0,credit_minor:600}],
  history:[],chart:{id:'chart',company_id:'company',name:'Primary',base_currency:'THB'},
  accounts:[{id:'expense',company_id:'company',chart_id:'chart',code:'510',name:'Spare cost',status:'active'},{id:'inventory',company_id:'company',chart_id:'chart',code:'110',name:'Inventory',status:'active'}],
  periods:[],dimensions:[],dimensionValues:[],
 };
 const reply=()=>({runtime:'rust',data:journalFlow,workspace,access:{sourceOnly:true,canPost:true,purchaseControls:{...purchaseControls,'finance.accounting':{newWork:false,settleExisting:false,history:false,revision:'source-only'}}},context});
 const legacy=()=>({runtime:'legacy',data:null,workspace:null,access:null,context});
 const commandRequest=(raw:string)=>new Request('https://acme.example.test/api/v1/holdings/acme/flows/journal/cost',{method:'POST',headers:{host:'acme.example.test',origin:'https://acme.example.test',cookie:'opaque','idempotency-key':'retain-field-cost-flow','x-erpx-context-company':'company','x-erpx-context-assignment':'grant','x-erpx-context-assignment-version':'3','x-erpx-context-revision':'7'},body:raw});
 it('uses a source-qualified native workspace/decisions and only an explicit empty legacy partition may select remaining consumers',async()=>{
  const {createNativeFieldJournalFlowReader}=await import('../../../packages/feature-finance/src/adapters/server/native-field-journal-flow');
  const transport=vi.fn(async(url:URL,headers:Headers)=>{expect(url.pathname).toBe('/api/v1/holdings/acme/finance/journals/cost/flow');expect(headers.get('x-erpx-context-company')).toBe('company');expect(headers.get('x-erpx-context-assignment-version')).toBe('3');expect(headers.get('cookie')).toBe('opaque');return json(reply());});
  const result=await createNativeFieldJournalFlowReader(JSON.stringify(config),transport)(actor,incoming,'acme','cost');
  expect(result?.workspace.lines.map(l=>l.debit_minor)).toEqual([600,0]);expect(result?.access.sourceOnly).toBe(true);expect(result?.flow.kind).toBe('journal');expect(transport).toHaveBeenCalledTimes(1);
  expect(await createNativeFieldJournalFlowReader(JSON.stringify(config),async()=>json(legacy()))(actor,incoming,'acme','cost')).toBeUndefined();
  await expect(createNativeFieldJournalFlowReader(JSON.stringify(config),async()=>json({...legacy(),workspace}))(actor,incoming,'acme','cost')).rejects.toMatchObject({code:'NATIVE_FINANCE_RESPONSE_INVALID'});
 });
 it('rejects stale identity, another Company and a relabelled native source instead of returning a legacy verdict',async()=>{
  const {createNativeFieldJournalFlowReader}=await import('../../../packages/feature-finance/src/adapters/server/native-field-journal-flow');
  for(const changed of [
   {...reply(),context:{...context,userId:'other'}},
   {...reply(),context:{...context,revision:8}},
   {...reply(),workspace:{...workspace,journal:{...workspace.journal,company_id:'other'}}},
   {...reply(),workspace:{...workspace,journal:{...workspace.journal,source_type:'manual'}}},
   {...reply(),data:{...journalFlow,steps:[{...step,primary:{...staff,userId:'withdrawn'}}]}},
  ])await expect(createNativeFieldJournalFlowReader(JSON.stringify(config),async()=>json(changed))(actor,incoming,'acme','cost')).rejects.toBeDefined();
  await expect(createNativeFieldJournalFlowReader(JSON.stringify(config),async()=>json({error:{code:'SOURCE_CHANGED'}},409))(actor,incoming,'acme','cost')).rejects.toMatchObject({status:409});
 });
 it('preserves original command bytes/key and reads a fresh native view after acknowledgement',async()=>{
  const {createNativeFieldJournalFlowCommands}=await import('../../../packages/feature-finance/src/adapters/server/native-field-journal-flow');
  let reads=0;const raw='{"stepKey":"draft","userId":"reader","expectedVersion":0,"recordVersion":1,"reason":"Retain original \\ud800 cost review","saveDefault":false,"source":"legacy"}';
  const read=async()=>json({...reply(),data:{...journalFlow,version:reads++===0?0:2}});
  const write=vi.fn(async(url:URL,headers:Headers,body:Uint8Array)=>{
   expect(url.pathname).toBe('/api/v1/holdings/acme/finance/journals/cost/flow');expect(headers.get('idempotency-key')).toBe('retain-field-cost-flow');expect(headers.get('x-erpx-context-revision')).toBe('7');expect(new TextDecoder().decode(body)).toBe(raw);
   return json({data:{id:'cost',version:2}});
  });
  const result=await createNativeFieldJournalFlowCommands(JSON.stringify(config),read,write)(commandRequest(raw),actor,'acme','cost');
  expect(result?.status).toBe(200);expect(await result?.json()).toMatchObject({data:{version:2,kind:'journal'},command:{id:'cost',version:2}});expect(reads).toBe(2);expect(write).toHaveBeenCalledTimes(1);
 });
 it('retains the acknowledged command as uncertain when fresh readback changes identity',async()=>{
  const {createNativeFieldJournalFlowCommands}=await import('../../../packages/feature-finance/src/adapters/server/native-field-journal-flow');
  let reads=0;const read=async()=>json(reads++===0?reply():{...reply(),context:{...context,revision:8}});
  const write=vi.fn(async()=>json({data:{id:'cost',version:2}}));
  const result=await createNativeFieldJournalFlowCommands(JSON.stringify(config),read,write)(commandRequest('{}'),actor,'acme','cost');
  expect(result?.status).toBe(503);expect(await result?.json()).toMatchObject({error:{code:'NATIVE_FINANCE_READBACK_UNCERTAIN'},command:{id:'cost',version:2}});expect(write).toHaveBeenCalledTimes(1);
 });
});

describe('native Field financial Work transport',()=>{
 const item={id:invoice.id,type:'invoice',actionKind:'work',title:'FS-order',detail:'Review',href:'/finance/invoices/'+invoice.id,status:'draft',updatedAt:1,queue:'waiting',flow:nativeFlow,canAct:false,category:'setup',setupReason:'Select a primary',relationships:[],nextAction:'Review',blocker:'Select a primary',responsibleQueue:'Finance'};
 const workActor={...actor,userId:context.userId,holdingId:context.holdingId,holdingSlug:'acme'} as WorkContext;
 it('forwards the captured identity to the finite native owner contribution',async()=>{
  const transport=vi.fn(async(url:URL,h:Headers)=>{expect(url.pathname).toBe('/api/v1/holdings/acme/finance/field-work');expect(h.get('cookie')).toBe('opaque');expect(h.get('host')).toBe('acme.example.test');expect(h.get('x-erpx-context-company')).toBe('company');expect(h.get('x-erpx-context-assignment')).toBe('grant');expect(h.get('x-erpx-context-revision')).toBe('7');expect(h.get('x-erpx-context-assignment-version')).toBe('3');return json({data:[item],context});});
  expect(await createNativeFieldFinancialWorkReader(JSON.stringify(config),transport)(incoming,workActor)).toEqual([item]);expect(transport).toHaveBeenCalledTimes(1);
 });
 it('never substitutes legacy or manufactures authority after stale or malformed native replies',async()=>{
  for(const value of [{data:[item],context:{...context,companyId:'other'}},{data:[{...item,canAct:true}],context},{data:[{...item,href:'https://foreign.invalid/'}],context},{data:[{...item,flow:{...nativeFlow,recordId:'other'}}],context},{data:[item,item],context},{data:[item],context,runtime:'legacy'},{data:[item],context:{...context,sessionId:'not-a-render-context-field'}}])await expect(createNativeFieldFinancialWorkReader(JSON.stringify(config),async()=>json(value))(incoming,workActor)).rejects.toBeInstanceOf(Error);
  for(const status of [401,403,404,409,503])await expect(createNativeFieldFinancialWorkReader(JSON.stringify(config),async()=>json({error:{code:'DENIED'}},status))(incoming,workActor)).rejects.toMatchObject({status});
 });
});

describe('native generic controlled payment transport',()=>{
 const paymentConfig=config;
 const payment={id:'payment-key',company_id:'company',kind:'receipt',status:'released',version:3,amount_minor:10000,outstanding_minor:10000,currency:'THB',created_by:'creator',settlement_invoice_id:'field-invoice-key'};
 const paymentData={payment,allocations:[],reversal:null,periods:[],invoices:[],bankTransactions:[],fxAccounts:[],sourcePurchase:{capability:'finance.service-invoice',settleExisting:true,history:true,revision:'actual-purchase'}};
 const purchaseControls=Object.fromEntries(['finance.accounting','finance.banking','finance.close','finance.advanced','finance.receivables','finance.tax','finance.service-invoice'].map(cap=>[cap,{newWork:false,settleExisting:cap==='finance.banking',history:cap==='finance.banking',revision:'current-purchase'}]));
 const request=(body:unknown)=>new Request('https://acme.example.test/api/payment',{method:'POST',headers:{host:'acme.example.test',origin:'https://acme.example.test',cookie:'opaque','idempotency-key':'generic-key','x-erpx-context-company':'company','x-erpx-context-assignment':'grant','x-erpx-context-revision':'7','x-erpx-context-assignment-version':'3'},body:JSON.stringify(body)});
 it('observes the invoice payment runtime rather than the invoice posting runtime',async()=>{
  const calls:string[]=[];
  const transport=vi.fn(async(url:URL,_headers:Headers,_body:Uint8Array,_signal:AbortSignal,method:'GET'|'POST'='POST')=>{
   calls.push(`${method} ${url.pathname}`);
   if(method==='GET')return json({data:{id:'bill-open',runtime:'rust'}});
   return json({data:{id:'pay-generic',status:'draft',version:1,settlementInvoiceId:'bill-open'}},201);
  });
  const input={kind:'payment',partyId:'supplier',paymentNumber:'PAY-1',paymentDate:'2026-01-10',currency:'THB',amount:'10.00',bankAccount:'bank',settlementInvoiceId:'bill-open'};
  const response=await createNativeServicePaymentCommands(JSON.stringify(config),transport)(request(input),actor,'acme',['payments'],input);
  expect(response?.status).toBe(201);
  expect(calls).toEqual(['GET /api/v1/holdings/acme/finance/invoices/bill-open/payment-runtime','POST /api/v1/holdings/acme/finance/payments']);
 });
 it('accepts a generic payable workspace with banking purchase controls',async()=>{
  const generic={...payment,kind:'payment',settlement_invoice_id:'bill-open'};
  const genericData={...paymentData,payment:generic,sourcePurchase:{capability:'finance.banking',settleExisting:true,history:true,revision:'actual-purchase'}};
  const read=createNativeServicePaymentReader(JSON.stringify(paymentConfig),async(url)=>json(url.pathname.endsWith('/runtime')?{data:{id:'payment-key',runtime:'rust'}}:{data:genericData,context,filesMode:'disabled',sourceOnly:false,purchaseControls}));
  expect((await read(actor,incoming,'acme','payment-key'))?.payment.kind).toBe('payment');
 });
});


describe('native Finance setup readiness transport',()=>{
 const setupReply={data:{charts:[{id:'chart',name:'Primary',base_currency:'THB'}],parties:[{id:'customer',code:'C-1',name:'Customer'}],checks:[{id:'chart',label:'Chart of accounts and base currency',complete:true,detail:'1 chart(s)'},{id:'approvers',label:'Independent Finance preparer and approver',complete:false,detail:'1 preparer(s), 1 approver(s) with current Company authority'}]},context};
 it('reads the exact setup view with the current rendered context and no extra header forwarding',async()=>{
  const calls:{url:URL;headers:Headers}[]=[];const transport=vi.fn(async(url:URL,headers:Headers)=>{calls.push({url,headers});return json(setupReply);});
  const result=await createNativeFinanceSetupWorkspaceReader(JSON.stringify(config),transport)(incoming,actor,'acme');
  expect(calls).toHaveLength(1);expect(calls[0]!.url.pathname).toBe('/api/v1/holdings/acme/finance/charts/setup/accounts');expect(calls[0]!.url.search).toBe('?view=setup');
  expect([...calls[0]!.headers.keys()].sort()).toEqual(['accept','cache-control','cookie','host','x-erpx-context-assignment','x-erpx-context-assignment-version','x-erpx-context-company','x-erpx-context-revision']);
  expect(result.checks[0]!.detail).toBe('1 chart(s)');
 });
 it('rejects changed context or malformed checks instead of synthesizing setup facts',async()=>{
  const changed=structuredClone(setupReply);changed.context.companyId='foreign';
  await expect(createNativeFinanceSetupWorkspaceReader(JSON.stringify(config),async()=>json(changed))(incoming,actor,'acme')).rejects.toMatchObject({code:'NATIVE_FINANCE_CONTEXT_CHANGED'});
  const malformed=structuredClone(setupReply);malformed.data.checks[0]!.detail='';
  await expect(createNativeFinanceSetupWorkspaceReader(JSON.stringify(config),async()=>json(malformed))(incoming,actor,'acme')).rejects.toMatchObject({code:'NATIVE_FINANCE_SETUP_RESPONSE_INVALID'});
  await expect(createNativeFinanceSetupWorkspaceReader(JSON.stringify(config),async()=>json(null,503))(incoming,actor,'acme')).rejects.toMatchObject({code:'NATIVE_FINANCE_SETUP_UNAVAILABLE',status:503});
 });
 it('keeps configured native failures closed and branch setup explicitly legacy',async()=>{
  const previous=process.env.ERPX_NATIVE_FINANCE_SETUP_CONFIG;
  try{
   delete process.env.ERPX_NATIVE_FINANCE_SETUP_CONFIG;expect(await nativeFinanceSetupWorkspace(actor,'acme')).toBeUndefined();
   process.env.ERPX_NATIVE_FINANCE_SETUP_CONFIG='{broken';await expect(nativeFinanceSetupWorkspace(actor,'acme')).rejects.toBeInstanceOf(NativeFinanceReadError);
   process.env.ERPX_NATIVE_FINANCE_SETUP_CONFIG=JSON.stringify(config);
   const branch={...actor,scope:{kind:'branch',branchSlug:'bangkok'}} as typeof actor;
   expect(await nativeFinanceSetupWorkspace(branch,'acme')).toBeUndefined();
  }finally{if(previous===undefined)delete process.env.ERPX_NATIVE_FINANCE_SETUP_CONFIG;else process.env.ERPX_NATIVE_FINANCE_SETUP_CONFIG=previous;}
 });
});
