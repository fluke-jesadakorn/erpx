import {createRequire} from 'node:module';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
if(process.version!=='v26.11.1')throw Error('NODE_26_11_1_REQUIRED');
const require=createRequire(resolve('package.json')),ts=require('typescript'),zod=require('zod');
const revision='a5ed8737a2f05eff1ee9841fda4a094b3926d732';
const paths=['packages/server-field_service/src/domain/service.ts','packages/server-field_service/src/application/service.ts','packages/server-field_service/src/domain/package-commands.ts','packages/server-shared/src/domain/error.ts','packages/server-shared/src/domain/consumption-lot.ts'];
const sources=new Map(paths.map(path=>[path,execFileSync('git',['show',`${revision}:${path}`],{encoding:'utf8',maxBuffer:2*1024*1024})]));
for(const [path,original]of sources)if(readFileSync(path,'utf8')!==original)throw Error('SOURCE_CHANGED:'+path);
function compile(path,bindings){const ownerModule={exports:{}};const code=ts.transpileModule(sources.get(path),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;new Function('module','exports','require',code)(ownerModule,ownerModule.exports,name=>{if(name==='zod')return zod;if(Object.hasOwn(bindings,name))return bindings[name];throw Error('UNREVIEWED_ORIGINAL_IMPORT:'+name);});return ownerModule.exports;}
const shared={...compile(paths[3],{}),...compile(paths[4],{})};
const domain=compile(paths[0],{'@erpx/server-shared/index':shared});
const application=compile(paths[1],{'@erpx/server-shared/index':shared,'@erpx/server-field_service/domain/service':domain});
const purchases=compile(paths[2],{});
const now=1_800_000_000_100;
const row={id:'order',company_id:'company',branch_id:'branch',department_id:null,version:7,status:'in_visit',technician_id:'actor',created_by:'creator',updated_at:42,scheduled_at:null,technician_eligible:1,can_review:0,issue:'Original issue',site:'Original site',exception_reason:null,agreement_id:'agreement',visit_rate_minor:10000,warranty_discount_bps:0,warranty_end:null,currency:'THB',agreement_status:'active',ticket_id:'ticket',customer_id:'customer'};
const issue={action:'issue-part',expectedVersion:7,reservationId:'reservation',quantityMilli:1000,reason:' Original reason ',allocations:[]};
const reserve={action:'reserve-part',expectedVersion:7,warehouseId:'warehouse',productId:'product',quantityMilli:1000,expiresAt:now+10000,reason:' Original reason '};
const reservation={id:'reservation',order_id:'order',warehouse_id:'warehouse',product_id:'product',quantity_milli:3000,consumed_milli:1000,released_milli:0,expires_at:now+10000};
const error=e=>({name:e.name,status:e.status??null,code:e.code??null,issues:e.issues?.map(i=>({code:i.code,path:i.path}))??null});
function schema(wire){try{return{normalized:JSON.stringify(domain.orderActionSchema.parse(JSON.parse(wire)))}}catch(e){return{error:error(e)}}}
function decision(wire,source,review){try{const input=domain.orderActionSchema.parse(JSON.parse(wire));return{result:domain.assertOrderAuthority(source,input,'actor',review)}}catch(e){return{error:error(e)}}}
async function observe(wire,source,options={}){
 const calls=[];const ports={actorId:'actor',actorUserIds:[],canRead:true,canReview:!!options.review,clock:{now:()=>{calls.push(['clock']);return now;}},
  authorize:async write=>{calls.push(['authorize',!!write]);if(options.permission===false)throw new shared.BusinessRuleError(403,'PERMISSION_REQUIRED','Synthetic current source denial');},authorizeReview:async()=>{calls.push(['authorizeReview']);},
  replay:async(key,operation,id,input)=>{calls.push(['replay',key,operation,id,JSON.stringify(input)]);return options.replay?{id:'order',version:8,status:'in_visit'}:null;},
  order:async id=>{calls.push(['order',id]);return source;},reservation:async(orderId,id)=>{calls.push(['reservation',orderId,id]);return options.noReservation?null:reservation;},
  prepareConsumption:async(order,res,input,key,time)=>{calls.push(['prepareConsumption',order.id,res.id,JSON.stringify(input),key,time]);if(options.consumptionError)throw new shared.BusinessRuleError(options.consumptionError==='MANUAL_RATE_REQUIRED'?422:409,options.consumptionError,'Synthetic original preparation refusal');return{effect:{marker:'synthetic-data-only'},id:'operational-part-original-key',valueMinor:200,journalId:'journal-original-key:parts'};},
  prepareBilling:async()=>{throw Error('OUT_OF_SCOPE_BILLING');},commit:async(m,result)=>{calls.push(['commit',m.operation,m.id,m.now,JSON.stringify(m.input),JSON.stringify(m.transition),m.consumption!==null]);return result;},
  workspace:async()=>{throw Error('OUT_OF_SCOPE_READ');},agreement:async()=>null,detail:async()=>{throw Error('OUT_OF_SCOPE_DETAIL');}};
 try{return{result:await application.createFieldServiceApplication(ports).changeOrder('order',JSON.parse(wire),'original-key'),calls};}catch(e){return{error:error(e),calls};}
}
const authority=[];
for(const action of [issue,reserve])for(const status of ['open','scheduled','dispatched','in_visit','exception','completed','cancelled','billed','unknown'])for(const own of [false,true])for(const eligible of [0,1])for(const review of [false,true])for(const canReview of [0,1]){
 const source={...row,status,technician_id:own?'actor':'other',technician_eligible:eligible,can_review:canReview};const wire=JSON.stringify(action);authority.push({id:`authority-${authority.length+1}`,source,wire,options:{review},decision:decision(wire,source,review),expected:await observe(wire,source,{review})});
}
const wireCases=[];
for(const [id,raw]of [['issue-default-allocation',(({allocations,...rest})=>rest)(issue)],['reserve',reserve],['unknown-strip',{...issue,unknown:'removed'}],['null-date',{...issue,entryDate:null}],['valid-date',{...issue,entryDate:'2026-02-28'}],['invalid-date',{...issue,entryDate:'2026-02-30'}],['lone-surrogate',{...issue,reason:'ab\ud800'}],['astral-limit',{...issue,reason:'😀'.repeat(3000)}],['astral-too-long',{...issue,reason:'😀'.repeat(3001)}],['unsafe-version',{...issue,expectedVersion:9007199254740992}],['max-version',{...issue,expectedVersion:9007199254740991}],['zero-quantity',{...issue,quantityMilli:0}],['null-allocations',{...issue,allocations:null}],['too-many-allocations',{...issue,allocations:Array.from({length:201},()=>({}))}],['invalid-lot',{...issue,allocations:[{lotId:'',quantityMilli:0,balanceVersion:-1}]}]]){const wire=JSON.stringify(raw);wireCases.push({id,wire,expected:schema(wire)});}
for(const wire of ['{"action":"issue-part","expectedVersion":1.0,"reservationId":"reservation","quantityMilli":1e3,"reason":"reason"}','{"action":"issue-part","expectedVersion":0,"expectedVersion":7,"reservationId":"reservation","quantityMilli":1000,"reason":"reason"}','null','[]'])wireCases.push({id:`lexical-${wireCases.length+1}`,wire,expected:schema(wire)});
const flow=[];
for(const [id,input,source,options]of [['permission-first',null,null,{permission:false}],['replay-before-source',issue,null,{replay:true}],['missing-order',issue,null,{}],['missing-reservation',issue,row,{noReservation:true}],['stock-before-late-version',{...issue,expectedVersion:9007199254740991},row,{consumptionError:'PART_STOCK_UNAVAILABLE'}],['quote-before-late-version',{...issue,expectedVersion:9007199254740991},row,{consumptionError:'MANUAL_RATE_REQUIRED'}],['stale-provisional', {...issue,expectedVersion:6},row,{}],['reserve-expired',{...reserve,expiresAt:now},{...row,status:'scheduled'},{}],['reserve-future',reserve,{...row,status:'scheduled'},{}],['issue',issue,row,{}]]){const wire=JSON.stringify(input);flow.push({id,wire,source,options,expected:await observe(wire,source,options)});}
const purchase=[];for(const operation of ['field.order.reserve-part','field.order.issue-part'])for(const asset of [false,true])purchase.push({operation,asset,expected:purchases.serviceCommandPurchases(operation,asset)});
const output={version:1,acceptance:false,description:'Exact original Field Service parts policy/schema/control flow using synthetic mandatory application ports only; no current DB/worker/reservation authority or stock effect is established.',node:process.version,zod:require('zod/package.json').version,revision,sources:paths.map(path=>({path,sha256:createHash('sha256').update(sources.get(path)).digest('hex')})),authority,wireCases,flow,purchase};
const target=resolve('fixtures/field_service/rust-parts/conformance.v1.json');const bytes=JSON.stringify(output,null,2)+'\n';
if(process.argv.includes('--check')){if(readFileSync(target,'utf8')!==bytes)throw Error('FIELD_SERVICE_ORACLE_DRIFT');}else writeFileSync(target,bytes);
process.stdout.write(JSON.stringify({authority:authority.length,wire:wireCases.length,flow:flow.length,purchases:purchase.length,node:output.node,zod:output.zod})+'\n');
