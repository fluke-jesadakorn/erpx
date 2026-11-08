import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {
  createNativeCommerceOrdersClient,
  nativeCommerceOrdersAvailable,
  nativeCommerceOrdersConfiguration,
  nativeCommerceOrdersFailure,
  NativeCommerceOrdersError,
  type NativeCommerceOrdersPageContext,
  type NativeCommerceOrdersTransport,
} from '../../../packages/feature-commerce/src/public/native-commerce-orders';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44993',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=(overrides:Record<string,string>={})=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  ...overrides,
});
const actor:NativeCommerceOrdersPageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment',role:'owner',
  assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},
  modules:['sales'],iam:{permissions:['commerce.read','commerce.sales','commerce.approve']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:null,
  departmentId:null,branchSlug:null,departmentSlug:null,assignmentId:'assignment',
  assignmentVersion:3,revision:7,role:'owner',assignmentScopeKind:'holding',
  modules:['sales'],permissions:['commerce.read','commerce.sales','commerce.approve'],
  expiresAt:9_007_199_254_740_991};
const input={holdingSlug:'acme',expected:{companyId:'company',assignmentId:'assignment',
  revision:7,assignmentVersion:3}};
const order={id:'order-1',company_id:'company',branch_id:null,department_id:null,
  kind:'quote',number:'QT-1',party_id:'party-1',warehouse_id:null,currency:'THB',
  order_date:'2026-11-01',due_date:'2026-11-08',note:'',state:'draft',
  subtotal_minor:10000,tax_minor:700,source_id:null,version:1,created_by:'user',
  approved_by:null,created_at:1,updated_at:1,price_list_id:null,price_list_version:null,
  discount_reason:null,discount_approval_required:0,discount_approved_by:null,
  discount_approved_at:null};
const line={id:'line-1',order_id:'order-1',product_id:'product-1',description:'Service',
  quantity_milli:1000,fulfilled_milli:0,price_minor:10000,tax_bps:700,subtotal_minor:10000,
  tax_minor:700,ordinal:0,list_price_minor:null,discount_bps:0,item_type:'service',
  sku:'SVC-1',unit:'each',tracking_mode:'none'};
const document={order,lines:[line]};
const receipt={id:'order-1',state:'submitted',version:2,salesId:null};

function envelope(data:unknown) {
  return JSON.stringify({data,context});
}

function response(value:unknown, status=200) {
  return new Response(typeof value==='string'?value:JSON.stringify(value),{
    status,
    headers:{'content-type':'application/json'},
  });
}

function transportFor(
  handler:(method:string,url:URL,headers:Headers,body:string|undefined)=>Response,
):NativeCommerceOrdersTransport {
  return async (method,url,headers,body) => handler(method,url,headers,body);
}

describe('native Commerce order transport',()=>{
  it('keeps the selector closed until an exact configuration is present',()=>{
    expect(nativeCommerceOrdersAvailable()).toBe(false);
    expect(nativeCommerceOrdersConfiguration(config()).holdings).toHaveLength(1);
    for(const invalid of [
      config({privateBaseUrl:'http://127.0.0.1:3100'}),
      config({privateBaseUrl:'https://127.0.0.1:44993'}),
      config({privateBaseUrl:'http://localhost:44993'}),
      config({timeoutMs:200}),
      config({holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}]}),
      config({holdings:[
        {origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'},
        {origin:'https://acme.example.com',holdingSlug:'other',holdingId:'h002'},
      ]}),
      'not-json',
    ]) {
      expect(()=>createNativeCommerceOrdersClient(
        nativeCommerceOrdersConfiguration(invalid),
      )).toThrow(NativeCommerceOrdersError);
    }
  });

  it('forwards the exact target, session, scope and context for a document read',async()=>{
    const seen:unknown[]=[];
    const transport=transportFor((method,url,headers,body)=>{
      seen.push({method,path:url.pathname,host:headers.get('host'),
        cookie:headers.get('cookie'),branch:headers.get('x-erpx-workspace-branch'),
        company:headers.get('x-erpx-context-company'),body});
      return response(envelope(document));
    });
    const client=createNativeCommerceOrdersClient(
      nativeCommerceOrdersConfiguration(config()),transport);
    const value=await client.detail(incoming(),actor,'order-1',input);
    expect(value).toEqual(document);
    expect(seen).toEqual([{
      method:'GET',path:'/api/v1/holdings/acme/commerce/orders/order-1',
      host:'acme.example.com',cookie:'__Host-erpx-product=session',branch:null,
      company:'company',body:undefined,
    }]);
  });

  it('rejects foreign/extra/duplicate/unordered documents and changed context',async()=>{
    const bad=(body:unknown)=>createNativeCommerceOrdersClient(
      nativeCommerceOrdersConfiguration(config()),
      transportFor(()=>response(envelope(body))),
    );
    await expect(bad({...document,order:{...order,company_id:'foreign'}})
      .detail(incoming(),actor,'order-1',input))
      .rejects.toMatchObject({code:'NATIVE_COMMERCE_ORDER_RESPONSE_INVALID'});
    await expect(bad({...document,order:{...order,extra:true}})
      .detail(incoming(),actor,'order-1',input))
      .rejects.toMatchObject({code:'NATIVE_COMMERCE_ORDER_RESPONSE_INVALID'});
    await expect(bad({order,lines:[line,line]})
      .detail(incoming(),actor,'order-1',input))
      .rejects.toMatchObject({code:'NATIVE_COMMERCE_ORDER_RESPONSE_INVALID'});
    await expect(bad({order,lines:[{...line,ordinal:1},{...line,id:'line-2',product_id:'product-2',ordinal:0}]})
      .detail(incoming(),actor,'order-1',input))
      .rejects.toMatchObject({code:'NATIVE_COMMERCE_ORDER_RESPONSE_INVALID'});
    await expect(createNativeCommerceOrdersClient(
      nativeCommerceOrdersConfiguration(config()),
      transportFor(()=>response(JSON.stringify({data:document,
        context:{...context,companyId:'foreign'}}))),
    ).detail(incoming(),actor,'order-1',input))
      .rejects.toMatchObject({code:'NATIVE_COMMERCE_ORDER_CONTEXT_CHANGED'});
    await expect(createNativeCommerceOrdersClient(
      nativeCommerceOrdersConfiguration(config()),
      transportFor(()=>response(envelope(document))),
    ).detail(incoming({host:'evil.example.com'}),actor,'order-1',input))
      .rejects.toMatchObject({code:'NATIVE_COMMERCE_ORDER_AUTHORITY_INVALID'});
  });

  it('forwards one exact transition body, key and content type',async()=>{
    let seen:unknown;
    const transport=transportFor((_method,_url,headers,body)=>{
      seen={body,key:headers.get('idempotency-key'),content:headers.get('content-type')};
      return response(envelope(receipt));
    });
    const client=createNativeCommerceOrdersClient(
      nativeCommerceOrdersConfiguration(config()),transport);
    const value=await client.transition(
      incoming(),actor,'order-1',
      {action:'submit',expectedVersion:1},'commerce-order-command-0001',input);
    expect(value).toEqual(receipt);
    expect(seen).toEqual({
      body:'{"action":"submit","expectedVersion":1}',
      key:'commerce-order-command-0001',
      content:'application/json',
    });
  });

  it('forwards one exact save body and key for create and update',async()=>{
    const seen:unknown[]=[];
    const transport=transportFor((_method,url,headers,body)=>{
      seen.push({path:url.pathname,body,key:headers.get('idempotency-key'),content:headers.get('content-type')});
      return response(envelope({id:'order-1',version:2}));
    });
    const client=createNativeCommerceOrdersClient(
      nativeCommerceOrdersConfiguration(config()),transport);
    const save={
      kind:'quote' as const,partyId:'party-1',warehouseId:'',orderDate:'2026-11-10',
      dueDate:'2026-11-15',note:'',priceListId:'list-1',
      discountReason:'Reviewed commercial discount',
      lines:[{productId:'product-1',quantity:'1',price:'80',taxBps:700}],
      expectedVersion:1,
    };
    expect(await client.save(incoming(),actor,null,save,'commerce-save-0001',input))
      .toEqual({id:'order-1',version:2});
    expect(await client.save(incoming(),actor,'order-1',save,'commerce-save-0002',input))
      .toEqual({id:'order-1',version:2});
    const body='{"kind":"quote","partyId":"party-1","warehouseId":"","orderDate":"2026-11-10","dueDate":"2026-11-15","note":"","priceListId":"list-1","discountReason":"Reviewed commercial discount","lines":[{"productId":"product-1","quantity":"1","price":"80","taxBps":700}],"expectedVersion":1}';
    expect(seen).toEqual([
      {path:'/api/v1/holdings/acme/commerce/orders',body,key:'commerce-save-0001',content:'application/json'},
      {path:'/api/v1/holdings/acme/commerce/orders/order-1',body,key:'commerce-save-0002',content:'application/json'},
    ]);
    await expect(client.save(incoming(),actor,null,{...save,lines:[]},'commerce-save-0003',input))
      .rejects.toMatchObject({code:'NATIVE_COMMERCE_ORDER_REQUEST_INVALID'});
  });

  it('maps owner failures and never falls back after a configured failure',async()=>{
    const owner=createNativeCommerceOrdersClient(
      nativeCommerceOrdersConfiguration(config()),
      transportFor(()=>response(JSON.stringify({
        error:{code:'COMMERCE_ORDER_CHANGED',message:'The order changed. Reload and try again.'},
      }),409)),
    );
    await expect(owner.transition(incoming(),actor,'order-1',
      {action:'submit',expectedVersion:1},'commerce-order-command-0001',input))
      .rejects.toMatchObject({code:'COMMERCE_ORDER_CHANGED',status:409});

    const unavailable=createNativeCommerceOrdersClient(
      nativeCommerceOrdersConfiguration(config()),
      transportFor(()=>{throw new Error('socket unavailable');}),
    );
    await expect(unavailable.detail(incoming(),actor,'order-1',input))
      .rejects.toMatchObject({code:'NATIVE_COMMERCE_ORDER_UNAVAILABLE'});

    const malformed=createNativeCommerceOrdersClient(
      nativeCommerceOrdersConfiguration(config()),
      transportFor(()=>response('not-json')),
    );
    await expect(malformed.detail(incoming(),actor,'order-1',input))
      .rejects.toMatchObject({code:'NATIVE_COMMERCE_ORDER_RESPONSE_INVALID'});
  });

  it('maps changed context and request failures to the retained page policy',()=>{
    expect(nativeCommerceOrdersFailure(
      new NativeCommerceOrdersError('NATIVE_COMMERCE_ORDER_CONTEXT_CHANGED'),
    )).toEqual({
      status:409,
      code:'WORK_CONTEXT_CHANGED',
      message:'Your work role changed. Reload and review before retrying.',
    });
    expect(nativeCommerceOrdersFailure(
      new NativeCommerceOrdersError('NATIVE_COMMERCE_ORDER_REQUEST_INVALID'),
    )).toMatchObject({status:422,code:'COMMERCE_ORDER_REQUEST_INVALID'});
    expect(nativeCommerceOrdersFailure(
      new NativeCommerceOrdersError('NATIVE_COMMERCE_ORDER_CONFIGURATION_INVALID'),
    )).toMatchObject({status:503,code:'NATIVE_COMMERCE_ORDER_UNAVAILABLE'});
    expect(nativeCommerceOrdersFailure(new Error('unrelated'))).toBeUndefined();
  });
});
