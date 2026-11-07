import {beforeAll,beforeEach,afterEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from '../../support/runtime-env';
import {expenseDb as db,expenseFixture,key} from '../../support/helpers/expense-fixture';
import {saveMaster} from '@erpx/server-commerce/public/commerce';
import {saveCount} from '@erpx/server-inventory/public/inventory-counts';
import {inventoryCountApplication} from '@erpx/server-inventory/adapters/inventory-counts';
import {withCountPurchasedCommand} from '@erpx/server-inventory/adapters/count-purchases';
import {countAttachmentPurchaseScope,authorizeCountAttachmentPurchase} from '@erpx/server-inventory/adapters/count-attachment-purchases';
import {serviceCountPersistedMarker,serviceCountWriterOwner,serviceCountWriterOwnerInDatabase} from '@erpx/server-inventory/adapters/service-initial-physical-stock-count-source';
vi.mock('@erpx/server-runtime/adapters/runtime',()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();});
afterEach(()=>vi.unstubAllEnvs());
it('requires the owned unit and rejects persisted malformed Service markers before any legacy effects',async()=>{
 const id=`count-${key()}`,command=key();
 await expect(inventoryCountApplication(db).saveCount(f.owner,id,{warehouseId:'missing',countDate:'2026-09-02',reason:'Check owned transaction',expectedVersion:1,lines:[{productId:'missing',countedQuantity:'0',allocations:[]}]},command)).rejects.toThrow('TRANSACTION_NOT_ACTIVE_OR_OWNED');
 const rollback=new Error('fixture rollback');
 await expect(db.transaction(async tx=>{
  await tx.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?, 'inventory.count.create', ?, 'test', ?, ?, 1, ?)").bind(key(),f.owner.userId,id,JSON.stringify({_serviceInitialPhysicalStockCount:{kind:'malformed'}}),JSON.stringify({companyId:'foreign-company'})).run();
  for(const result of ['[]']){
   const bad=`count-${key()}`;await tx.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?, 'inventory.count.create', ?, 'test', ?, ?, 1, ?)").bind(key(),f.owner.userId,bad,result,JSON.stringify({companyId:f.company})).run();
   expect(await serviceCountPersistedMarker(tx,bad)).toBe(true);await expect(serviceCountWriterOwnerInDatabase(tx,f.owner,bad)).rejects.toMatchObject({status:404,code:'COUNT_NOT_FOUND'});
  }
  expect(await serviceCountPersistedMarker(tx,id)).toBe(true);
  await expect(serviceCountWriterOwnerInDatabase(tx,f.owner,id)).rejects.toMatchObject({status:404,code:'COUNT_NOT_FOUND'});
  await expect(inventoryCountApplication(tx).saveCount(f.owner,id,{warehouseId:'missing',countDate:'2026-09-02',reason:'Check native ownership',expectedVersion:1,lines:[{productId:'missing',countedQuantity:'0',allocations:[]}]},command)).rejects.toMatchObject({code:'COUNT_NOT_FOUND'});
  let ran=false;await expect(withCountPurchasedCommand(tx,f.owner,{key:command,operation:'flow.assign',id,purpose:'settle-existing',permission:'inventory.read',existing:true,flowStep:'submitted'},async()=>{ran=true;return {}; })).rejects.toMatchObject({status:404,code:'COUNT_NOT_FOUND'});expect(ran).toBe(false);
  expect(await tx.prepare('SELECT count(*)::int n FROM command_receipts WHERE command_id=?').bind(command).first('n')).toBe(0);
  throw rollback;
 })).rejects.toBe(rollback);
 expect(await serviceCountWriterOwner(f.owner,id)).toBe('other');
 await expect(db.transaction(async tx=>{await tx.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?, 'inventory.count.create', ?, 'test', ?, NULL, 1, ?)").bind(key(),f.owner.userId,`count-${key()}`,JSON.stringify({companyId:f.company})).run();})).rejects.toMatchObject({code:'23514'});
});
it('keeps ordinary Count create and idempotent replay inside a real transaction when purchases are disabled',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','disabled');
 const warehouse=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Fence warehouse'},key())).id);
 const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Fence product',price:'10'},key())).id);
 const input={warehouseId:warehouse,countDate:'2026-09-02',reason:'Ordinary Count stays available',lines:[{productId:product,countedQuantity:'0',allocations:[]}]},command=key();
 const first=await saveCount(f.owner,null,input,command);expect(await saveCount(f.owner,null,input,command)).toEqual(first);
 expect(await serviceCountWriterOwner(f.owner,String(first.id))).toBe('other');
});

it('quarantines a malformed Count receipt before disabled-mode Files purchase fallback',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','disabled');
 const warehouse=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Files boundary warehouse'},key())).id);
 const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Files boundary product',price:'10'},key())).id);
 const creationKey=key(),created=await saveCount(f.owner,null,{warehouseId:warehouse,countDate:'2026-09-02',reason:'Actual ordinary Count for Files',lines:[{productId:product,countedQuantity:'0',allocations:[]}]},creationKey),id=String(created.id),rollback=new Error('fixture rollback');
 await expect(db.transaction(async tx=>{
  await tx.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?, 'inventory.count.create', ?, 'test', ?, '[]', 1, ?)").bind(key(),f.owner.userId,id,JSON.stringify({companyId:f.company})).run();
  await expect(countAttachmentPurchaseScope(tx,f.owner,'stock_count',id,'history')).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});
  await expect(authorizeCountAttachmentPurchase(tx,f.owner,'stock_count',id,'read')).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});
  throw rollback;
 })).rejects.toBe(rollback);
 const account=await db.prepare('SELECT id FROM finance_accounts WHERE company_id=? AND chart_id=? LIMIT 1').bind(f.company,f.chart).first<string>('id');if(!account)throw Error('FINANCE_ACCOUNT_FIXTURE_REQUIRED');
 await expect(db.transaction(async tx=>{
  const setup=key();await tx.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?, 'fixture.noop', ?, 'test', ?, '{}', 1, ?)").bind(setup,f.owner.userId,f.company,JSON.stringify({companyId:f.company})).run();
  await tx.prepare("INSERT INTO inventory_service_initial_physical_sources(count_id,company_id,branch_id,warehouse_id,product_id,count_command_id,service_setup_command_id,chart_id,inventory_account_id,variance_account_id,currency,proof_json,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?, 'THB','{}'::jsonb,?,1)").bind(id,f.company,f.branch,warehouse,product,creationKey,setup,f.chart,account,account,f.owner.userId).run();
  await expect(countAttachmentPurchaseScope(tx,f.owner,'stock_count',id,'history')).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});
  await expect(authorizeCountAttachmentPurchase(tx,f.owner,'stock_count',id,'read')).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});
  throw rollback;
 })).rejects.toBe(rollback);
 expect(await db.prepare('SELECT version FROM inventory_counts WHERE id=?').bind(id).first('version')).toBe(1);
});
