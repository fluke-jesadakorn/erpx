import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {
  createNativeInventoryReceiptReader,
  createNativeInventoryStockRegisterReader,
} from '../../../packages/feature-inventory/src/adapters/server/native-stock-register';
import type {NativeAvailabilityContext} from '../../../packages/feature-inventory/src/adapters/server/native-availability';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44991',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const headers=()=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  origin:'https://acme.example.com',
});
const actor:NativeAvailabilityContext={
  userId:'user-1',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment',revision:7,assignmentVersion:3,
};
const context={...actor,userId:'user-1',holdingId:'h001',companyId:'company',branchId:null,departmentId:null,
  branchSlug:null,departmentSlug:null,assignmentScopeKind:'holding',role:'holding_owner',modules:['inventory'],permissions:['inventory.read'],expiresAt:9_007_199_254_740_991};
const stock={warehouseId:'w',productId:'p',sku:'SKU',name:'Product',unit:'pcs',trackingMode:'lot',
  warehouseName:'Warehouse',quantityMilli:3000,reservedMilli:0,quarantinedMilli:1000,
  inTransitMilli:0,availableMilli:2000,valueMinor:3000,version:2};
const lot={id:'lot-1',warehouseId:'w',productId:'p',sku:'SKU',trackingMode:'lot',warehouseName:'Warehouse',
  lotCode:'L-1',serialNumber:null,manufacturedDate:null,expiryDate:'2027-01-01',status:'active',
  quantityMilli:3000,valueMinor:3000,version:1,qualityHoldMilli:1000,qualityStatus:'held'};
const count={id:'count-1',number:'CNT-1',status:'posted',version:4,countDate:'2026-11-04',reason:'Opening count',
  warehouseId:'w',warehouseName:'Warehouse',netValueMinor:0,postedAt:100,journalId:'journal-1',
  initialPhysicalSource:true,createdAt:1,updatedAt:2};
const totals={trackedQuantityMilli:3000,trackedValueMinor:3000,lotQuantityMilli:3000,lotValueMinor:3000,consistent:true};
const registerOk=()=>new Response(JSON.stringify({context,data:{stock:[stock],lots:[lot],counts:[count],totals,currency:'THB'}}),{status:200,headers:{'content-type':'application/json'}});
const order={id:'order-1',number:'PO-1',kind:'purchase',state:'confirmed',version:3,warehouseId:'w',
  branchId:'b',departmentId:null,orderDate:'2026-11-01',currency:'THB',partyName:'Supplier'};
const receiptLine={id:'line-1',productId:'p',sku:'SKU',description:'Product',unit:'pcs',trackingMode:'lot',
  quantityMilli:5000,fulfilledMilli:1000,remainingMilli:4000,priceMinor:2500,taxBps:0,ordinal:1,
  stock:{quantityMilli:3000,valueMinor:3000,version:2,reservedMilli:0,quarantinedMilli:1000},
  lots:[{id:'lot-1',lotCode:'L-1',serialNumber:null,status:'active',quantityMilli:3000,valueMinor:3000,version:1,qualityHoldMilli:1000,qualityStatus:'held'}],
  lotsTruncated:false};
const receiptOk=()=>new Response(JSON.stringify({context,data:{companyId:'company',order,lines:[receiptLine]}}),{status:200,headers:{'content-type':'application/json'}});

describe('native Inventory stock-register transport',()=>{
 it('forwards the configured Holding with the original session and CAS',async()=>{
  const transport=vi.fn().mockResolvedValue(registerOk());
  const read=createNativeInventoryStockRegisterReader(config(),transport);
  const result=await read(headers(),'acme',actor);
  expect(result.currency).toBe('THB');
  expect(result.stock[0]).toMatchObject({warehouse_id:'w',quantity_milli:3000,quarantined_milli:1000,available_milli:2000});
  expect(result.lots[0]).toMatchObject({id:'lot-1',lot_code:'L-1',quality_hold_milli:1000,quality_status:'held'});
  expect(result.counts[0]).toMatchObject({id:'count-1',initial_physical_source:true,version:4,status:'posted'});
  expect(result.totals).toEqual({tracked_quantity_milli:3000,tracked_value_minor:3000,lot_quantity_milli:3000,lot_value_minor:3000,consistent:true});
  const [url,out]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44991/api/v1/holdings/acme/inventory/stock-register');
  expect(out.get('cookie')).toBe('__Host-erpx-product=session');
  expect(out.get('x-erpx-context-company')).toBe('company');
  expect(out.get('x-erpx-context-assignment')).toBe('assignment');
  expect(out.get('x-erpx-context-revision')).toBe('7');
  expect(out.get('x-erpx-context-assignment-version')).toBe('3');
  expect(out.get('origin')).toBe('https://acme.example.com');
 });

 it('rejects an unknown Holding, a changed context or an inconsistent reconciliation',async()=>{
  const transport=vi.fn();
  const read=createNativeInventoryStockRegisterReader(config(),transport);
  await expect(read(headers(),'other',actor)).rejects.toMatchObject({code:'NATIVE_INVENTORY_STOCK_AUTHORITY_INVALID'});
  const foreign=headers();foreign.set('host','evil.example.com');
  await expect(read(foreign,'acme',actor)).rejects.toMatchObject({code:'NATIVE_INVENTORY_STOCK_AUTHORITY_INVALID'});
  expect(transport).not.toHaveBeenCalled();
  const changed=vi.fn().mockResolvedValue(new Response(JSON.stringify({context:{...context,revision:8},data:{stock:[stock],lots:[lot],counts:[count],totals,currency:'THB'}}),{status:200,headers:{'content-type':'application/json'}}));
  await expect(createNativeInventoryStockRegisterReader(config(),changed)(headers(),'acme',actor))
    .rejects.toMatchObject({code:'NATIVE_INVENTORY_STOCK_RESPONSE_INVALID'});
  const mismatch=vi.fn().mockResolvedValue(new Response(JSON.stringify({context,data:{stock:[stock],lots:[lot],counts:[count],totals:{...totals,consistent:false},currency:'THB'}}),{status:200,headers:{'content-type':'application/json'}}));
  await expect(createNativeInventoryStockRegisterReader(config(),mismatch)(headers(),'acme',actor))
    .rejects.toMatchObject({code:'NATIVE_INVENTORY_STOCK_RESPONSE_INVALID'});
 });

 it('admits loopback 3101 only with the explicit production opt-in',()=>{
  expect(()=>createNativeInventoryStockRegisterReader(config({privateBaseUrl:'http://127.0.0.1:3101'}),vi.fn())).toThrow();
  expect(()=>createNativeInventoryStockRegisterReader(config({privateBaseUrl:'http://127.0.0.1:3101',production:true}),vi.fn())).not.toThrow();
 });
});

describe('native Inventory receipt transport',()=>{
 it('maps the current purchase line and lot facts without inventing authority',async()=>{
  const transport=vi.fn().mockResolvedValue(receiptOk());
  const read=createNativeInventoryReceiptReader(config(),transport);
  const result=await read(headers(),'acme',actor,'order-1');
  expect(result.can_receive).toBe(true);
  expect(result.order).toMatchObject({number:'PO-1',kind:'purchase',version:3,warehouse_id:'w'});
  expect(result.lines[0]).toMatchObject({sku:'SKU',remaining_milli:4000,tracking_mode:'lot',lots_truncated:false});
  expect(result.lines[0].stock).toMatchObject({version:2,quarantined_milli:1000});
  expect(result.lines[0].lots[0]).toMatchObject({lot_code:'L-1',quality_hold_milli:1000});
  const [url,out]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44991/api/v1/holdings/acme/inventory/receipts/order-1');
  expect(out.get('x-erpx-context-company')).toBe('company');
  expect(out.get('cookie')).toBe('__Host-erpx-product=session');
 });

 it('fails closed on an overlong order id or an unavailable response',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response('{}',{status:503,headers:{'content-type':'application/json'}}));
  const read=createNativeInventoryReceiptReader(config(),transport);
  await expect(read(headers(),'acme',actor,'x'.repeat(201))).rejects.toMatchObject({code:'NATIVE_INVENTORY_RECEIPT_SELECTION_INVALID'});
  expect(transport).not.toHaveBeenCalled();
  await expect(read(headers(),'acme',actor,'order-1')).rejects.toMatchObject({code:'NATIVE_INVENTORY_RECEIPT_UNAVAILABLE',status:503});
 });
});
