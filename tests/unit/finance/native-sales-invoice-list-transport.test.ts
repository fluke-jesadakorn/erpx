import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {createNativeSalesInvoiceListReader} from '../../../packages/feature-finance/src/adapters/server/native-sales-invoice-list';
import type {NativeJournalPageContext} from '../../../packages/feature-finance/src/adapters/server/native-journal-page';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44993',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const headers=()=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  origin:'https://acme.example.com',
});
const actor:NativeJournalPageContext={
  userId:'user-1',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment',role:'holding_owner',
  assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},
  modules:['finance'],iam:{permissions:['finance.read']},
};
const context={...actor,userId:'user-1',holdingId:'h001',companyId:'company',branchId:null,departmentId:null,
  branchSlug:null,departmentSlug:null,assignmentScopeKind:'holding',role:'holding_owner',assignmentVersion:3,revision:7,
  modules:['finance'],permissions:['finance.read'],expiresAt:9_007_199_254_740_991};
const invoice={id:'inv-1',number:'INV-1',date:'2026-01-01',status:'posted',currency:'THB',
  party_name:'Acme Customer',page_key:'100',outstanding_minor:5000};
const ok=()=>new Response(JSON.stringify({
  context,
  data:{
    query:{tab:'invoices',q:'',status:''},
    summary:[{currency:'THB',awaiting_minor:8000,overdue_minor:5000}],
    received:[{currency:'THB',received_minor:2000}],
    page:{items:[invoice],nextCursor:null,previousCursor:null,hasMore:false},
    permissionRequired:null,
  },
}),{status:200,headers:{'content-type':'application/json'}});

describe('native Finance sales-invoice list transport',()=>{
 it('forwards the configured Holding with the original session and CAS',async()=>{
  const transport=vi.fn().mockResolvedValue(ok());
  const read=createNativeSalesInvoiceListReader(config(),transport);
  const result=await read(headers(),actor,'acme',{tab:'invoices',status:'posted'});
  expect(result.summary[0]).toEqual({currency:'THB',awaiting_minor:8000,overdue_minor:5000});
  expect(result.received[0]).toEqual({currency:'THB',received_minor:2000});
  expect(result.page.items[0]).toMatchObject({id:'inv-1',number:'INV-1',party_name:'Acme Customer'});
  const [url,out]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/finance/sales/invoices?tab=invoices&status=posted');
  expect(out.get('cookie')).toBe('__Host-erpx-product=session');
  expect(out.get('x-erpx-context-company')).toBe('company');
  expect(out.get('x-erpx-context-assignment')).toBe('assignment');
  expect(out.get('x-erpx-context-revision')).toBe('7');
  expect(out.get('x-erpx-context-assignment-version')).toBe('3');
  expect(out.get('origin')).toBe('https://acme.example.com');
 });

 it('rejects an unknown Holding or host before any request',async()=>{
  const transport=vi.fn();
  const read=createNativeSalesInvoiceListReader(config(),transport);
  await expect(read(headers(),actor,'other',{tab:'invoices'}))
    .rejects.toMatchObject({code:'NATIVE_FINANCE_AUTHORITY_INVALID'});
  const foreign=headers();foreign.set('host','evil.example.com');
  await expect(read(foreign,actor,'acme',{tab:'invoices'}))
    .rejects.toMatchObject({code:'NATIVE_FINANCE_AUTHORITY_INVALID'});
  expect(transport).not.toHaveBeenCalled();
 });

 it('rejects a changed rendered context or a malformed page',async()=>{
  const changed=vi.fn().mockResolvedValue(new Response(JSON.stringify({
    context:{...context,revision:8},
    data:{query:{tab:'invoices'},summary:[],received:[],page:{items:[],nextCursor:null,previousCursor:null,hasMore:false},permissionRequired:null},
  }),{status:200,headers:{'content-type':'application/json'}}));
  await expect(createNativeSalesInvoiceListReader(config(),changed)(headers(),actor,'acme',{tab:'invoices'}))
    .rejects.toMatchObject({code:'NATIVE_FINANCE_CONTEXT_CHANGED'});
  const malformed=vi.fn().mockResolvedValue(new Response(JSON.stringify({
    context,
    data:{query:{tab:'invoices'},summary:[],received:[],page:{items:[{id:'inv-1'}],nextCursor:null,previousCursor:null,hasMore:false},permissionRequired:null},
  }),{status:200,headers:{'content-type':'application/json'}}));
  await expect(createNativeSalesInvoiceListReader(config(),malformed)(headers(),actor,'acme',{tab:'invoices'}))
    .rejects.toMatchObject({code:'NATIVE_FINANCE_SALES_RESPONSE_INVALID'});
 });
});
