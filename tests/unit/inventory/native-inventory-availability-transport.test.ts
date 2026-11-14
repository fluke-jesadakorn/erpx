import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {createNativeInventoryAvailabilityReader,createNativeInventoryMovementsReader,type NativeAvailabilityContext} from '../../../packages/feature-inventory/src/adapters/server/native-availability';

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
const row={warehouseId:'w',productId:'p',sku:'SKU',name:'Product',unit:'pcs',trackingMode:'none',
  warehouseName:'Warehouse',quantityMilli:10000,reservedMilli:3000,quarantinedMilli:2000,
  inTransitMilli:1500,availableMilli:7000,valueMinor:1000,version:1};
const ok=()=>new Response(JSON.stringify({context,data:[row],currency:'THB'}),{status:200,headers:{'content-type':'application/json'}});
const movement={id:'m1',warehouseId:'w',productId:'p',sku:'SKU',warehouseName:'Warehouse',quantityMilli:1000,valueMinor:100,movementDate:'2026-11-04',sourceType:'pos_sale',sourceId:'source-1',orderNumber:'POS-1',fulfillmentDate:'2026-11-04',createdAt:100};
const movementsOk=()=>new Response(JSON.stringify({context,data:[movement],nextCursor:'100:m1'}),{status:200,headers:{'content-type':'application/json'}});

describe('native Inventory availability transport',()=>{
 it('forwards the configured Holding with the original session and CAS',async()=>{
  const transport=vi.fn().mockResolvedValue(ok());
  const read=createNativeInventoryAvailabilityReader(config(),transport);
  const result=await read(headers(),'acme',actor);
  expect(result.currency).toBe('THB');
  expect(result.rows).toEqual([{warehouse_id:'w',product_id:'p',sku:'SKU',name:'Product',unit:'pcs',
    tracking_mode:'none',warehouse_name:'Warehouse',quantity_milli:10000,reserved_milli:3000,
    quarantined_milli:2000,in_transit_milli:1500,available_milli:7000,value_minor:1000,version:1}]);
  const [url,out]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44991/api/v1/holdings/acme/inventory/availability');
  expect(out.get('cookie')).toBe('__Host-erpx-product=session');
  expect(out.get('x-erpx-context-company')).toBe('company');
  expect(out.get('x-erpx-context-assignment')).toBe('assignment');
  expect(out.get('x-erpx-context-revision')).toBe('7');
  expect(out.get('x-erpx-context-assignment-version')).toBe('3');
  expect(out.get('origin')).toBe('https://acme.example.com');
 });

 it('rejects an unknown Holding or host before any request',async()=>{
  const transport=vi.fn();
  const read=createNativeInventoryAvailabilityReader(config(),transport);
  await expect(read(headers(),'other',actor)).rejects.toMatchObject({code:'NATIVE_INVENTORY_AVAILABILITY_AUTHORITY_INVALID'});
  const foreign=headers();foreign.set('host','evil.example.com');
  await expect(read(foreign,'acme',actor)).rejects.toMatchObject({code:'NATIVE_INVENTORY_AVAILABILITY_AUTHORITY_INVALID'});
  expect(transport).not.toHaveBeenCalled();
 });

 it('admits loopback 3101 only with the explicit production opt-in',()=>{
  expect(()=>createNativeInventoryAvailabilityReader(config({privateBaseUrl:'http://127.0.0.1:3101'}),vi.fn())).toThrow();
  expect(()=>createNativeInventoryAvailabilityReader(config({privateBaseUrl:'http://127.0.0.1:3101',production:true}),vi.fn())).not.toThrow();
 });

 it('rejects a changed rendered context or a derived availability mismatch',async()=>{
  const changed=vi.fn().mockResolvedValue(new Response(JSON.stringify({context:{...context,revision:8},data:[row],currency:'THB'}),{status:200,headers:{'content-type':'application/json'}}));
  await expect(createNativeInventoryAvailabilityReader(config(),changed)(headers(),'acme',actor))
    .rejects.toMatchObject({code:'NATIVE_INVENTORY_AVAILABILITY_CONTEXT_CHANGED'});
  const mismatch=vi.fn().mockResolvedValue(new Response(JSON.stringify({context,data:[{...row,availableMilli:9999}],currency:'THB'}),{status:200,headers:{'content-type':'application/json'}}));
  await expect(createNativeInventoryAvailabilityReader(config(),mismatch)(headers(),'acme',actor))
    .rejects.toMatchObject({code:'NATIVE_INVENTORY_AVAILABILITY_RESPONSE_INVALID'});
 });
});

describe('native Inventory movement transport',()=>{
 it('forwards the cursor and maps the all-source movement page',async()=>{
  const transport=vi.fn().mockResolvedValue(movementsOk());
  const read=createNativeInventoryMovementsReader(config(),transport);
  const result=await read(headers(),'acme',actor,'99:m0');
  expect(result.nextCursor).toBe('100:m1');
  expect(result.rows[0]).toMatchObject({id:'m1',source_type:'pos_sale',order_id:'source-1',order_number:'POS-1',quantity_milli:1000});
  const [url,out]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44991/api/v1/holdings/acme/inventory/movements?cursor=99%3Am0');
  expect(out.get('x-erpx-context-company')).toBe('company');
  expect(out.get('cookie')).toBe('__Host-erpx-product=session');
 });

 it('rejects a changed context or an overlong cursor before returning rows',async()=>{
  const changed=vi.fn().mockResolvedValue(new Response(JSON.stringify({context:{...context,revision:8},data:[movement],nextCursor:null}),{status:200,headers:{'content-type':'application/json'}}));
  await expect(createNativeInventoryMovementsReader(config(),changed)(headers(),'acme',actor))
    .rejects.toMatchObject({code:'NATIVE_INVENTORY_MOVEMENT_CONTEXT_CHANGED'});
  const transport=vi.fn();
  await expect(createNativeInventoryMovementsReader(config(),transport)(headers(),'acme',actor,'x'.repeat(401)))
    .rejects.toMatchObject({code:'NATIVE_INVENTORY_MOVEMENT_CURSOR_INVALID'});
  expect(transport).not.toHaveBeenCalled();
 });
});
