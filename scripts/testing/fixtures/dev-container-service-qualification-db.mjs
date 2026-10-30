// Guest-only readback of an actual UI-created Count and native worker projection.
import fs from 'node:fs';
import {Pool} from 'pg';
const [id]=process.argv.slice(2),path='/workspace/erpx/.runtime/local/config/private-api.json';
if(!fs.existsSync('/.dockerenv')||process.cwd()!=='/workspace/erpx'||!/^count-[A-Za-z0-9_-]{1,180}$/.test(id??'')||(fs.statSync(path).mode&0o077)!==0)throw Error('PRIVATE_GUEST_CASE_REQUIRED');
const config=JSON.parse(fs.readFileSync(path,'utf8')),holding=config.holdings.find(x=>x.holdingId==='h001'&&x.slug==='acme');
function checked(database){const u=new URL(database.url);if(u.hostname!=='127.0.0.1'||u.port!=='5432'||u.username!=='erpx_test_admin'||!/^\/[a-z0-9_]+$/.test(u.pathname)||u.pathname!==`/erpx_dev_rust_${database.leaseToken}`||!/^[0-9a-f]{32}$/.test(database.leaseToken)||u.search||u.hash)throw Error('EXACT_OWNED_LEASE_REQUIRED');return u.href;}
const write=new Pool({connectionString:checked(holding.database),max:1}),read=new Pool({connectionString:checked(holding.readDatabase),max:1});
try{
 let event;for(let i=0;i<100;i++){const result=await write.query("SELECT o.id,o.aggregate_version,o.processed_at,o.last_error,c.company_id,c.version,c.status FROM outbox_events o JOIN inventory_counts c ON c.id=o.aggregate_id WHERE o.aggregate_id=$1 AND o.aggregate_type='inventory_count' AND o.event_type='inventory.count.create'",[id]);if(result.rowCount===1&&result.rows[0].processed_at!==null){event=result.rows[0];break;}await new Promise(resolve=>setTimeout(resolve,200));}
 if(!event||event.last_error!==null||event.company_id!=='acme-co'||Number(event.version)!==1||event.status!=='draft'||Number(event.aggregate_version)!==1)throw Error('NATIVE_COUNT_ACK_REQUIRED');
 const receipts=await read.query('SELECT count(*)::int AS n FROM projection_receipts WHERE event_id=$1 AND aggregate_id=$2 AND aggregate_version=1',[event.id,id]),checkpoint=await read.query('SELECT count(*)::int AS n FROM projection_checkpoints WHERE last_event_id=$1',[event.id]);
 if(receipts.rows[0].n!==1||checkpoint.rows[0].n!==1)throw Error('NATIVE_PROJECTION_REQUIRED');
 console.log(JSON.stringify({company:'acme-co',countId:id,countVersion:1,acknowledged:true,receiptCount:1,checkpointCount:1}));
}finally{await write.end();await read.end();}
