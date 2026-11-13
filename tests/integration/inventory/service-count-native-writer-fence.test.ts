import {beforeAll,beforeEach,afterEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from '../../support/runtime-env';
import {expenseDb as db,expenseFixture,key} from '../../support/helpers/expense-fixture';
import {saveCount,transitionCount} from '@erpx/server-inventory/public/inventory-counts';
import {serviceCountPersistedMarker,serviceCountWriterOwner,serviceCountWriterOwnerInDatabase} from '@erpx/server-inventory/adapters/service-initial-physical-stock-count-source';
import {withCountPurchasedCommand} from '@erpx/server-inventory/adapters/count-purchases';
vi.mock('@erpx/server-runtime/adapters/runtime',()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();});
afterEach(()=>vi.unstubAllEnvs());
it('retires the TypeScript Count writer behind the native command boundary',async()=>{
 await expect(saveCount(f.owner,null,{warehouseId:'missing',countDate:'2026-09-02',reason:'Check native boundary',lines:[]},key())).rejects.toMatchObject({status:503,code:'NATIVE_INVENTORY_COUNT_REQUIRED'});
 await expect(transitionCount(f.owner,'count-key','submit',{expectedVersion:1,attachmentSetIds:[]},key())).rejects.toMatchObject({status:503,code:'NATIVE_INVENTORY_COUNT_REQUIRED'});
 expect(await db.prepare('SELECT count(*)::int n FROM inventory_counts WHERE company_id=?').bind(f.company).first('n')).toBe(0);
});
it('rejects persisted malformed Service markers before any owner effects',async()=>{
 const id=`count-${key()}`,command=key();
 const rollback=new Error('fixture rollback');
 await expect(db.transaction(async tx=>{
  await tx.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?, 'inventory.count.create', ?, 'test', ?, ?, 1, ?)").bind(key(),f.owner.userId,id,JSON.stringify({_serviceInitialPhysicalStockCount:{kind:'malformed'}}),JSON.stringify({companyId:'foreign-company'})).run();
  for(const result of ['[]']){
   const bad=`count-${key()}`;await tx.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?, 'inventory.count.create', ?, 'test', ?, ?, 1, ?)").bind(key(),f.owner.userId,bad,result,JSON.stringify({companyId:f.company})).run();
   expect(await serviceCountPersistedMarker(tx,bad)).toBe(true);await expect(serviceCountWriterOwnerInDatabase(tx,f.owner,bad)).rejects.toMatchObject({status:404,code:'COUNT_NOT_FOUND'});
  }
  expect(await serviceCountPersistedMarker(tx,id)).toBe(true);
  await expect(serviceCountWriterOwnerInDatabase(tx,f.owner,id)).rejects.toMatchObject({status:404,code:'COUNT_NOT_FOUND'});
  let ran=false;await expect(withCountPurchasedCommand(tx,f.owner,{key:command,operation:'flow.assign',id,purpose:'settle-existing',permission:'inventory.read',existing:true,flowStep:'submitted'},async()=>{ran=true;return {}; })).rejects.toMatchObject({status:404,code:'COUNT_NOT_FOUND'});expect(ran).toBe(false);
  expect(await tx.prepare('SELECT count(*)::int n FROM command_receipts WHERE command_id=?').bind(command).first('n')).toBe(0);
  throw rollback;
 })).rejects.toBe(rollback);
 expect(await serviceCountWriterOwner(f.owner,id)).toBe('other');
});
