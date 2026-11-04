import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {createNativeFinanceBankCommand} from '../../../packages/feature-finance/src/adapters/server/native-finance-bank';
const config=JSON.stringify({privateBaseUrl:'http://127.0.0.1:44993',timeoutMs:250,holdings:[{origin:'https://acme.example.test',holdingSlug:'acme',holdingId:'h001'}]});
const actor={userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',branchId:null,departmentId:null,assignmentId:'assignment',role:'finance',assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},modules:['finance'],iam:{permissions:['finance.write','finance.approve']}} as never;
const request=(body:unknown)=>new Request('https://acme.example.test/api/finance',{method:'POST',headers:{host:'acme.example.test',origin:'https://acme.example.test',cookie:'opaque','idempotency-key':'bank-key','x-erpx-context-company':'company','x-erpx-context-assignment':'assignment','x-erpx-context-revision':'7','x-erpx-context-assignment-version':'3'},body:JSON.stringify(body)});
const json=(value:unknown,status=200)=>new Response(JSON.stringify(value),{status,headers:{'content-type':'application/json'}});
describe('native Finance bank transport',()=>{
 it('imports multi-row statements and reconciles through the exact native paths',async()=>{
  const calls:string[]=[];const transport=vi.fn(async(url:URL,_h:Headers,body:Uint8Array,_s:AbortSignal,method:'GET'|'POST'='POST')=>{calls.push(`${method} ${url.pathname}`);const input=JSON.parse(new TextDecoder().decode(body));return url.pathname.endsWith('/import')?json({data:{id:`statement-${'bank-key'}`,status:'imported',imported:input.rows.length,settlementPaymentId:null}},201):json({data:{id:'bankrow-1',status:'matched',version:input.expectedVersion+1,matchedType:'payment',matchedId:input.matchedId}});});
  const command=createNativeFinanceBankCommand(config,transport);
  const input={bankAccount:'bank',statementNumber:'ST-1',statementDate:'2026-01-31',opening:'0.00',closing:'15.00',sourceHash:'source-hash',rows:[{externalId:'E-1',transactionDate:'2026-01-05',amount:'15.00',currency:'THB'}]};
  const imported=await command(request(input),actor,'acme',['bank','statements','import'],input);
  expect(imported?.status).toBe(201);
  const reconcile={expectedVersion:1,matchedType:'payment',matchedId:'payment-1',proof:'Reviewed the row',attachmentSetIds:[]};
  const matched=await command(request(reconcile),actor,'acme',['bank','transactions','bankrow-1','reconcile'],reconcile);
  expect(matched?.status).toBe(200);
  expect(calls).toEqual(['POST /api/v1/holdings/acme/finance/bank/statements/import','POST /api/v1/holdings/acme/finance/bank/transactions/bankrow-1/reconcile']);
 });
 it('returns a closed native error and never silently falls back',async()=>{
  const command=createNativeFinanceBankCommand(config,async()=>json({error:{code:'STATEMENT_UNBALANCED',message:'Opening plus movements must equal closing.'}},422));
  const response=await command(request({}),actor,'acme',['bank','statements','import'],{});
  expect(response?.status).toBe(422);
  expect(await response?.json()).toMatchObject({error:{code:'STATEMENT_UNBALANCED'}});
  const missing=await createNativeFinanceBankCommand(config,async()=>json({data:{}}))(request({}),actor,'acme',['bank','statements','import'],{});
  expect(missing?.status).toBe(502);
 });
 it('leaves the legacy path only when the selector is absent',async()=>{
  delete process.env.ERPX_NATIVE_FINANCE_BANK_CONFIG;
  const {nativeFinanceBankCommand}=await import('../../../packages/feature-finance/src/adapters/server/native-finance-bank');
  expect(await nativeFinanceBankCommand(request({}),actor,'acme',['bank','statements','import'],{})).toBeUndefined();
 });
});
