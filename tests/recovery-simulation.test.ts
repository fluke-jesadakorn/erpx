import {beforeAll,it,expect,vi} from 'vitest';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {mkdir,mkdtemp,readFile,rm,writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
import {Pool} from 'pg';
import {PostgresDatabase} from '../src/server/runtime/postgres';
import {applyMigrations,env} from './runtime-env';
import {expenseDb as db,expenseFixture,key} from './helpers/expense-fixture';
import {deliveryEvidence} from './helpers/delivery-evidence';
import {saveMaster,saveOrder,orderSchema,transitionOrder,getOrder,changeOrderReservation,fulfillOrder} from '../src/server/commerce';
vi.mock('../src/server/runtime',()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
const run=promisify(execFile),hash=(value:string|Buffer)=>createHash('sha256').update(value).digest('hex');
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
async function tableDigests(database:PostgresDatabase){
 const tables=(await database.pool.query("SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename")).rows as {tablename:string}[];
 const results:Record<string,{count:number;sha256:string}>={};
 for(const {tablename} of tables){if(!/^[a-z_][a-z0-9_]*$/.test(tablename))throw new Error('UNSAFE_TABLE_NAME');
  const rows=(await database.pool.query(`SELECT row_to_json(t)::text payload FROM "${tablename}" t ORDER BY row_to_json(t)::text`)).rows;
  results[tablename]={count:rows.length,sha256:hash(JSON.stringify(rows))};
 }return results;
}
it('restores an isolated source-linked business snapshot without reposting its transactions',async()=>{
 const adminUrl=new URL(process.env.ERPX_TEST_ADMIN_URL!);
 if(adminUrl.hostname!=='127.0.0.1'||adminUrl.port!=='5432'||adminUrl.username!=='erpx_test_admin'||adminUrl.pathname!=='/postgres')throw new Error('LOCAL_TEST_ADMIN_REQUIRED');
 const source=String(await db.prepare('SELECT current_database() name').first('name'));
 if(!/^erpx_test_[a-f0-9]{10}_h001_db$/.test(source))throw new Error('ISOLATED_SOURCE_REQUIRED');
 const target=source.replace('_h001_db','_restore'),directory=await mkdtemp(join(process.cwd(),'.runtime/tests/recovery-')),archive=join(directory,'holding.dump');
 const f=await expenseFixture();f.head.role='sales_manager';f.head.branchId=null;f.head.departmentId=null;f.head.assignmentId=await f.grant(f.head,'sales_manager');
 const warehouse=await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Restore rehearsal warehouse'},key());
 const product=await saveMaster(f.owner,'products',null,{sku:key(),name:'Restore rehearsal product',price:'10'},key());
 const party=await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Restore rehearsal customer'},key());
 await db.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,2000,1200,1)').bind(warehouse.id,product.id).run();
 const order=await saveOrder(f.owner,null,orderSchema.parse({kind:'sales',partyId:party.id,warehouseId:warehouse.id,orderDate:'2026-09-02',dueDate:'2026-09-03',lines:[{productId:product.id,quantity:'2',price:'10',taxBps:0}]}),key()),orderId=String(order.id);
 await transitionOrder(f.owner,orderId,{action:'submit',expectedVersion:1},key());await transitionOrder(f.head,orderId,{action:'approve',expectedVersion:2},key());
 const details=await getOrder(f.owner,orderId),lineId=String(details.lines[0].id);
 await changeOrderReservation(f.owner,orderId,{expectedVersion:3,action:'reserve',lineId,quantity:'2',expiresAt:Date.now()+3600000,reason:'Reserve before recovery rehearsal'},key());
 const delivery=await fulfillOrder(f.owner,orderId,{attachmentSetIds:[],expectedVersion:4,date:'2026-09-02',lines:[{lineId,quantity:'1',allocations:[]}],...(await deliveryEvidence(db,f.owner,orderId))},key());
 const expected=await tableDigests(db);
 const cliEnv={...process.env,PGHOST:adminUrl.hostname,PGPORT:adminUrl.port,PGUSER:decodeURIComponent(adminUrl.username),PGPASSWORD:decodeURIComponent(adminUrl.password),PGDATABASE:source};
 const admin=new Pool({connectionString:adminUrl.href,max:1});let restored:PostgresDatabase|undefined,created=false;
 try{
  const dumpVersion=(await run('pg_dump',['--version'],{env:cliEnv})).stdout.trim();
  const restoreVersion=(await run('pg_restore',['--version'],{env:cliEnv})).stdout.trim();
  expect(dumpVersion).toContain('18.6');expect(restoreVersion).toContain('18.6');
  await run('pg_dump',['--format=custom','--no-owner','--no-privileges','--file',archive],{env:cliEnv,timeout:120000});
  const archiveSha256=hash(await readFile(archive));
  const started=performance.now();
  await admin.query(`CREATE DATABASE "${target}" TEMPLATE template0`);created=true;
  await run('pg_restore',['--exit-on-error','--transaction-size=64','--no-owner','--no-privileges','--dbname',target,archive],{env:{...cliEnv,PGDATABASE:target},timeout:120000});
  const targetUrl=new URL(adminUrl);targetUrl.pathname=target;restored=new PostgresDatabase({connectionString:targetUrl.href});
  expect(await tableDigests(restored)).toEqual(expected);
  expect(await restored.prepare('SELECT state,version FROM commerce_orders WHERE id=?').bind(orderId).first()).toEqual({state:'partially_fulfilled',version:5});
  expect(await restored.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,product.id).first()).toEqual({quantity_milli:1000,value_minor:600});
  expect(await restored.prepare('SELECT SUM(quantity_milli) n FROM inventory_active_reservations WHERE warehouse_id=?').bind(warehouse.id).first('n')).toBe(1000);
  await expect(restored.prepare('UPDATE commerce_stock SET quantity_milli=0 WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,product.id).run()).rejects.toMatchObject({message:'STOCK_RESERVED'});
  const elapsedMs=Math.round(performance.now()-started);
  const evidence={kind:'isolated_postgres_restore',sourceId:orderId,fulfillmentId:delivery.fulfillmentId,tables:Object.keys(expected).length,archiveSha256,tableDigestSha256:hash(JSON.stringify(expected)),elapsedMs,scope:'Single holding database simulation; excludes files/control databases/WAL/provider recovery and production RPO/RTO sign-off'};
  const evidenceDirectory=join(process.cwd(),'.scratch/cross-department-audit');await mkdir(evidenceDirectory,{recursive:true});await writeFile(join(evidenceDirectory,'recovery-latest.json'),JSON.stringify(evidence,null,2)+'\n');
 }finally{
  await restored?.close();
  if(created){const owner=(await admin.query('SELECT pg_get_userbyid(datdba) owner FROM pg_database WHERE datname=$1',[target])).rows[0]?.owner;if(owner!=='erpx_test_admin')throw new Error('RESTORE_TARGET_OWNER_CHANGED');await admin.query(`DROP DATABASE "${target}"`);}
  await admin.end();await rm(directory,{recursive:true,force:true});
 }
},180000);
