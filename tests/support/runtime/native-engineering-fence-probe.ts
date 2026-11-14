import assert from 'node:assert/strict';
import {Pool} from 'pg';
import {PostgresDatabase} from '@erpx/server-runtime/adapters/runtime/postgres';
// Only the companion network-none Docker qualification supplies this lease.
const token=process.env.ERPX_FENCE_TOKEN;if(!token||!/^[a-f0-9]{32}$/.test(token))throw Error('OWNED_PROBE_LEASE_REQUIRED');
const name=`erpx_dev_rust_${token}`,lease=`erpx-rust-test:${token}`,url=`postgresql://erpx_test_admin:synthetic@127.0.0.1:5432/${name}`;
const marker='/*erpx-native-engineering-fence*/',sql=`SELECT ${marker} shobj_description(oid,'pg_database') AS lease FROM pg_database WHERE datname=current_database()`;
let current=true;const db=new PostgresDatabase({connectionString:url,max:2},{current:()=>current});const writer=new Pool({connectionString:url,max:1});const bare=new PostgresDatabase({connectionString:url,max:1});
try{
 const identity=(await writer.query("SELECT current_database() name,current_user::text role,session_user::text session_role,pg_get_userbyid(d.datdba) owner,shobj_description(d.oid,'pg_database') lease,r.rolsuper,r.rolcreaterole,has_table_privilege(current_user,'pg_catalog.pg_database','MAINTAIN') db_maintain,has_table_privilege(current_user,'pg_catalog.pg_shdescription','MAINTAIN') description_maintain,has_table_privilege(current_user,'pg_catalog.pg_database','UPDATE') db_update,has_table_privilege(current_user,'pg_catalog.pg_shdescription','UPDATE') description_update FROM pg_database d JOIN pg_roles r ON r.rolname=current_user WHERE d.datname=current_database()")).rows[0];
 assert.deepEqual(identity,{name,role:'erpx_test_admin',session_role:'erpx_test_admin',owner:'erpx_test_admin',lease,rolsuper:false,rolcreaterole:false,db_maintain:true,description_maintain:true,db_update:false,description_update:false});
 await assert.rejects(()=>bare.prepare(sql).first(),/NATIVE_ENGINEERING_FENCE_REQUIRED/);await assert.rejects(()=>db.pool.query(sql),/NATIVE_ENGINEERING_FENCE_REQUIRED/);
 await assert.rejects(()=>db.transaction(async tx=>{await tx.exec('SELECT 1; COMMIT');}),e=>(e as {code?:string}).code==='42601');
 await assert.rejects(()=>db.transaction(async tx=>{await tx.exec('/*leading comment*/ COMMIT');await tx.prepare(sql).first();}),/NATIVE_ENGINEERING_TRANSACTION_CONTROL_UNSUPPORTED/);
 await assert.rejects(()=>db.transaction(async tx=>{await tx.exec('/* outer /* nested */ comment */ COMMIT');}),/NATIVE_ENGINEERING_TRANSACTION_CONTROL_UNSUPPORTED/);
 await assert.rejects(()=>db.transaction(async tx=>{await tx.exec('SET TRANSACTION ISOLATION LEVEL READ COMMITTED');}),/NATIVE_ENGINEERING_TRANSACTION_CONTROL_UNSUPPORTED/);
 await db.transaction(async tx=>{assert.equal(await tx.prepare(sql).first('lease'),lease);const c=await writer.connect();try{await c.query("SET lock_timeout='400ms'");await assert.rejects(()=>c.query(`COMMENT ON DATABASE "${name}" IS 'changed'`),e=>(e as {code?:string}).code==='55P03');}finally{c.release();}assert.equal(await tx.prepare(sql).first('lease'),lease);});
 await writer.query(`COMMENT ON DATABASE "${name}" IS 'changed'`);assert.equal(await db.prepare(sql).first('lease'),'changed');await writer.query(`COMMENT ON DATABASE "${name}" IS '${lease}'`);
 await writer.query('CREATE TABLE fence_effect (id int)');await assert.rejects(()=>db.transaction(async tx=>{await tx.prepare('INSERT INTO fence_effect(id) VALUES(?)').bind(1).run();current=false;}),/NATIVE_ENGINEERING_ATTESTATION_CHANGED/);current=true;assert.equal(Number(await db.prepare('SELECT count(*) n FROM fence_effect').first('n')),0);
 // A retained pool client keeps its original query wrapper across module reloads.
 // The refreshed database must grant that wrapper the same fence capability.
 const reloadPath='/tmp/native-fence-reloaded.js';const reloaded=await import(reloadPath);
 reloaded.refreshPostgresReadInstrumentation(db);
 await db.transaction(async tx=>{assert.equal(await tx.prepare(sql).first('lease'),lease);await assert.rejects(()=>tx.exec('COMMIT'),/NATIVE_ENGINEERING_TRANSACTION_CONTROL_UNSUPPORTED/);});
 console.log('NATIVE_FENCE_ACTUAL_ADAPTER_PASS');
}finally{await bare.close();await writer.end();await db.close();}
