// Retained Field Service source + actual runtime batch compiler, fake query sink.
// This proves neither database execution nor the complete adapter's behavior.
import {createRequire} from 'node:module';
import {createHash} from 'node:crypto';
import {readFileSync,writeFileSync} from 'node:fs';
import {strict as assert} from 'node:assert';
assert.equal(process.version,'v26.11.1');
const paths=['packages/server-runtime/src/adapters/runtime/postgres.ts','packages/server-field_service/src/adapters/service.ts','packages/server-field_service/src/application/service.ts','packages/server-workflow/src/application/commands.ts'];
const expected=['3f613b91e9251f4568d37e8e29a9aecd786b581dd624d9914edf1876d486b2ba','5276d8703ffd87144a9bec9a5fb4d7246e9a42411029dc2cf5e1362a57ce2b5e','95aa03b1a0662783476429056fb5f7c5e568b95747830f9a8ac934e7ba1429c7','665c8e957951a7748c0211e58b2001a133d4479bf546fc986f61d1cc6920ea98'];
const sources=paths.map((path,i)=>{const text=readFileSync(path,'utf8');const sha256=createHash('sha256').update(text).digest('hex');assert.equal(sha256,expected[i],path);return{path,sha256,text}});
assert.match(sources[1].text,/sourceVersion:input\.expectedVersion\+1/);
assert.match(sources[1].text,/service-order-event-\$\{key\}/);
assert.match(sources[1].text,/input\.expectedVersion \+ 1/);
const require=createRequire(import.meta.url);const ts=require('typescript');
const code=ts.transpileModule(sources[0].text,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const m={exports:{}};new Function('module','exports','require',code)(m,m.exports,n=>n==='@erpx/server-runtime/adapters/runtime/read-metrics'?{}:require(n));
async function observe(version,reason){let queries=0;const db=m.exports.transactionDatabase({query:async()=>{queries++;return{rows:[],rowCount:0,fields:[]}}});try{await db.batch([db.prepare('SELECT ?').bind(reason),db.prepare('SELECT ?').bind(version+1)]);return{ok:true,queries}}catch(e){return{ok:false,name:e.name,message:e.message,queries}}}
const safe=await observe(2,'Synthetic reason'),maximum=await observe(Number.MAX_SAFE_INTEGER,'Synthetic reason'),maximumNul=await observe(Number.MAX_SAFE_INTEGER,'Synthetic\u0000reason');
assert.deepEqual(safe,{ok:true,queries:2});assert.deepEqual(maximum,{ok:false,name:'RangeError',message:'DATABASE_NUMBER_OUT_OF_RANGE',queries:0});assert.deepEqual(maximumNul,maximum);
const output={version:1,revision:'10e85f0f7d5cc961c81542d96308fe9a7eced761',node:process.version,acceptance:false,description:'Retained Field Service source identities and actual PostgreSQL runtime precompiler with synthetic selected arguments; no adapter or database execution.',sources:sources.map(({path,sha256})=>({path,sha256})),observed:{safe,maximum,maximumNul},inference:'Retained Field Service event, notification, Workflow output and issue consumption bind expectedVersion+1; runtime validates a batch before query dispatch.'};
const target='fixtures/field_service/rust-parts/numeric.v1.json';if(process.argv.includes('--check'))assert.deepEqual(JSON.parse(readFileSync(target,'utf8')),output);else writeFileSync(target,JSON.stringify(output,null,2)+'\n');console.log('Field Service numeric source/runtime probe matched');
