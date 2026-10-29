const expectedSources={
  "packages/feature-service/src/routes/api/v1/holdings/[holdingSlug]/field-service/route.ts": "07ad895c2eaef855efcd6492b629a9ef5fe5502db44c7696cae130610feba877",
  "packages/server-runtime/src/adapters/http.ts": "70ce5f82e007f79946dd95c3fd7b1b052c2e847e0d38a6e0ffd402364ea858f6",
  "packages/server-shared/src/domain/error.ts": "ff9c315f159f7adcff264d6637617b80760f758c7cb0b19e1896f85bb51b798c",
  "packages/server-shared/src/domain/consumption-lot.ts": "dddcf06b3390953a58715a7c7888b888fa1069a9f82ae0cf675f8fc799aa543f",
  "packages/server-field_service/src/domain/service.ts": "9fc084338201fe2ee1f5864f49f8eb568f757f433154008c633755dd05636e28",
  "packages/server-field_service/src/application/service.ts": "95aa03b1a0662783476429056fb5f7c5e568b95747830f9a8ac934e7ba1429c7",
  "packages/server-field_service/src/adapters/service.ts": "5276d8703ffd87144a9bec9a5fb4d7246e9a42411029dc2cf5e1362a57ce2b5e",
  "packages/server-field_service/src/adapters/purchases.ts": "69d1e89e2f2d156ceb58f76a60cf6965eb5abbfc44a2d7de75f3e85f30602d8e",
  "packages/server-field_service/src/domain/package-commands.ts": "0c403ef49059bb02a5f5c45630723e6500dcd2ba3b41e0b60de8b434e5d8b8b7",
  "packages/server-workflow/src/application/commands.ts": "665c8e957951a7748c0211e58b2001a133d4479bf546fc986f61d1cc6920ea98",
  "packages/server-workflow/src/domain/commands.ts": "c610e8a5dcd72dec347f6c2e842d469cf37a5e0eac428d82dfe5f50c6c76f92b",
  "packages/feature-identity_access/src/lib/guest-account-compat.ts": "9cb447b63957b7a91f6bbc85f2074a531e5e8bb3e14dac7d58cd17d61b5b6e80",
  "packages/server-manage_erpx/src/adapters/public-origin.ts": "42da7a5939c001617d5cb082a0e4c1054a75c39e5cfb11009097b52c37f3e7d8",
  "packages/server-manage_erpx/src/domain/public-origin.ts": "27bf065b5c10b4f18b2f782db60190b3a21281c8084e766fc3ae5145e9d48b88",
  "packages/feature-field_service/src/lib/command.ts": "5e9c974dd8cb41c38f77d8876f465c8e0d549f2ca4eed0dd9bd74189711d5f83",
  "packages/feature-work_projection/src/lib/work-fetch.ts": "70ee5907223c3e7c5ce95ce0fb1eed1c1487f9bb3f638c8270d56251ed7ec56b",
  "packages/contracts/src/holding-fetch.ts": "cbe52d2a0d0ccacec4eb983523752c29d324e8b6d6955b5fb0608a2664cc9a55"
};
const requiredVectors=[
  "reserve-success",
  "issue-success",
  "bad-origin-missing-key-denied-auth-malformed-json",
  "origin-omitted",
  "origin-empty",
  "missing-key-denied-auth-malformed-json",
  "empty-key-denied-auth-malformed-json",
  "whitespace-key-real-Headers",
  "duplicate-origin-real-Headers",
  "duplicate-key-real-Headers",
  "denied-auth-malformed-json",
  "allowed-auth-malformed-json",
  "entity-request-with-parts-action-and-id",
  "entity-agreement-with-parts-action-and-id",
  "other-entity-with-parts-action-id",
  "nonstring-id-with-parts-action",
  "null-json",
  "array-json",
  "empty-string-order-id",
  "long220-order-id",
  "control-order-id",
  "lone-surrogate-order-id",
  "unicode-order-id",
  "bad-nonempty-key-valid-owner-and-quote",
  "bad-key-invalid-schema",
  "invalid-schema-denied-write",
  "bad-key-missing-owner",
  "bad-key-missing-quote",
  "trimmed-too-short-reason",
  "multiple-schema-errors-first-message",
  "issue-unknown-warehouse-product-stripped",
  "issue-default-allocations",
  "lone-surrogate-schema-and-detail-units"
];
import {createRequire} from 'node:module';
import {createHash} from 'node:crypto';
import {readFileSync,writeFileSync,realpathSync,readdirSync} from 'node:fs';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
const fixtureRoot=resolve(dirname(fileURLToPath(import.meta.url)),'../../../fixtures/field_service/rust-http');
if(process.env.TZ!=='Asia/Bangkok'||process.env.LANG!=='en_US.UTF-8'||process.env.LC_ALL!=='en_US.UTF-8'||Intl.DateTimeFormat().resolvedOptions().locale!=='en-US'||Intl.DateTimeFormat().resolvedOptions().timeZone!=='Asia/Bangkok')throw Error('PROFILE_DRIFT');
if(process.version!=='v26.11.1')throw Error('NODE_26_11_1_REQUIRED');
const require=createRequire(resolve('package.json')),ts=require('typescript'),zod=require('zod');
const wrapper=require('typescript/package.json'),engine=require('@typescript/old/package.json');
if(wrapper.name!=='@typescript/typescript6'||wrapper.version!=='6.0.2'||engine.name!=='typescript'||engine.version!=='6.0.3'||ts.version!=='6.0.3'||require('zod/package.json').version!=='4.6.7')throw Error('DEPENDENCY_VERSION_DRIFT');
const hash=x=>createHash('sha256').update(x).digest('hex');
const files=Object.fromEntries(Object.entries(expectedSources).map(([p,h])=>{const b=readFileSync(p);if(hash(b)!==h)throw Error('SOURCE_DRIFT '+p);return [p,b.toString('utf8')]}));
const mod=(path,bindings,source=files[path])=>{const m={exports:{}};const code=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;new Function('module','exports','require',code)(m,m.exports,n=>{if(n==='zod')return zod;if(Object.hasOwn(bindings,n))return bindings[n];throw Error('UNREVIEWED_IMPORT:'+path+':'+n)});return m.exports};
const src={error:'packages/server-shared/src/domain/error.ts',lot:'packages/server-shared/src/domain/consumption-lot.ts',domain:'packages/server-field_service/src/domain/service.ts',app:'packages/server-field_service/src/application/service.ts',owner:'packages/server-field_service/src/adapters/service.ts',purchases:'packages/server-field_service/src/adapters/purchases.ts',package:'packages/server-field_service/src/domain/package-commands.ts',workflow:'packages/server-workflow/src/application/commands.ts',workflowDomain:'packages/server-workflow/src/domain/commands.ts',guest:'packages/feature-identity_access/src/lib/guest-account-compat.ts',http:'packages/server-runtime/src/adapters/http.ts',origin:'packages/server-manage_erpx/src/domain/public-origin.ts',route:'packages/feature-service/src/routes/api/v1/holdings/[holdingSlug]/field-service/route.ts'};
const shared={...mod(src.error,{}),...mod(src.lot,{})};
const domain=mod(src.domain,{'@erpx/server-shared/index':shared});
const application=mod(src.app,{'@erpx/server-shared/index':shared,'@erpx/server-field_service/domain/service':domain});
const packages=mod(src.package,{}),guest=mod(src.guest,{}), workflowDomain=mod(src.workflowDomain,{'@erpx/server-shared/index':shared});
const workflow=mod(src.workflow,{'@erpx/feature-identity_access/lib/guest-account-compat':guest,'@erpx/server-shared/index':shared,'@erpx/server-workflow/domain/commands':workflowDomain});
const http=mod(src.http,{'@erpx/feature-work_projection/lib/destination-links':{canonicalDestinationLinks:x=>x}}),origin=mod(src.origin,{});
const utf16=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const actor={holdingId:'synthetic-holding',companyId:'company',branchId:'branch',departmentId:null,userId:'actor',assignmentId:'assignment',role:'technician',modules:['field_service']};
const now=1800000000100;
const row={id:'order',company_id:'company',branch_id:'branch',department_id:null,version:7,status:'in_visit',technician_id:'actor',created_by:'creator',updated_at:42,scheduled_at:null,technician_eligible:1,can_review:0,issue:'Original issue',site:'Original site',exception_reason:null,agreement_id:'agreement',visit_rate_minor:10000,warranty_discount_bps:0,warranty_end:null,currency:'THB',agreement_status:'active',ticket_id:'ticket',customer_id:'customer'};
const reserve={id:'order',action:'reserve-part',expectedVersion:7,warehouseId:'warehouse',productId:'product',quantityMilli:1000,expiresAt:now+10000,reason:'  reason  '};
const issue={id:'order',action:'issue-part',expectedVersion:7,reservationId:'reservation',quantityMilli:1000,reason:'  reason  ',allocations:[]};
let trace=[],options={};
const event=(name,...args)=>trace.push([name,...args]);
const fakeSql={prepare(sql){let args=[];return {bind(...x){args=x;return this},async first(){if(sql.includes('jsonb_build_object')){event('fake.source',args[0],utf16(String(args[0])));return options.noSource?null:{image:'fake-image',identity:'fake-identity',asset:false,state:'synthetic'};}if(sql.includes('command_receipts')){event('fake.ownerReceipt',args[0]);return null;}event('fake.currentQuery',sql.includes('assignments')?'module':'permission');return ((options.denyWrite&&sql.includes('permission:records.write'))||(options.denyRead&&sql.includes('permission:records.read')))?false:true}}},async batch(xs){event('fake.batch',xs.length);return []}};
const fakeBoundary={before:[],after:[],observe:()=>event('fake.boundary.observe')};
const purchaseBindings={
 'server-only':{},'node:crypto':{randomUUID:()=> 'fixed-uuid'},
 '@erpx/server-identity_access/public/record-source-authority':{assignmentReadSql:(_x,p)=>`permission:${p}`},
 '@erpx/server-identity_access/public/iam-store':{iamMutationGuards:()=>[]},
 '@erpx/server-identity_access/public/field-service-authority':{fieldServiceTechnicianPredicate:()=> 'fake-technician'},
 '@erpx/server-workflow/public/command-guards':{workContextGuards:()=>[],financialConstraintError:()=>null},
 '../../../../packages/server-runtime/src/public/runtime':{holdingResources:async()=>({write:{transaction:async f=>{event('fake.transaction');return f(fakeSql)}}})},
 '@erpx/server-runtime/public/runtime/types':{isTransactionalSqlDatabase:()=>true},
 '@erpx/server-runtime/public/http':http,
 '@erpx/server-finance/public/finance-transactions':{flushFinanceDeferredEffects:async()=>event('fake.flush')},
 '@erpx/server-finance/public/field-service-references':{fieldServiceCustomerSourceRows:'fake-customers',fieldServiceFinanceSourceReadLocks:()=>[]},
 '@erpx/server-assets/public/field-service-references':{fieldServiceAssetSourceRows:'fake-assets',fieldServiceAssetSourceReadLocks:()=>[]},
 '@erpx/server-support/public/field-service-sources':{fieldServiceTicketSourceRows:'fake-tickets',fieldServiceTicketSourceReadLocks:()=>[]},
 '@erpx/server-inventory/public/field-service-purchase-sources':{fieldServiceInventorySourceReadLocks:()=>[]},
 '@erpx/server-commercial/public/purchase-command-boundary':{servicePackageEnforcementMode:async()=>{event('fake.enforcement','enforced');return 'enforced'},companyPurchaseReadLocks:()=>[{},{}],checkPurchasedCapability:async(_db,_m,v)=>event('fake.purchase',v.capability,v.purpose),purchaseCommandBoundary:(_db,_m,_k,v)=>{event('fake.purchaseBoundary',v.capability,v.purpose);return fakeBoundary},purchaseCommandFailure:()=>null},
 '@erpx/server-field_service/domain/package-commands':packages};
const purchased=mod(src.purchases,purchaseBindings);
function createWorkflow(){return workflow.createCommandApplication({repository:{receipt:async key=>{event('fake.workflowReceipt',key);return null},clearAssertions:()=>({}),insertReceipt:()=>({}),captureAssignmentAuthority:()=>({}),audit:()=>({}),outbox:()=>({})},clock:{now:()=>now},identifiers:{next:()=> 'fixed-id'},digest:async()=>{event('fake.digest');return 'fake-digest'},workContextGuards:()=>[],iamMutationGuards:()=>[],assignmentGuard:()=>({}),prepareOwnerStatements:async()=>{event('fake.workflowStatements');return []},batch:async()=>{event('fake.workflowBatch')},activeContext:async()=>null,activeGrant:async()=>null})}
function syntheticApplication(a){const w=createWorkflow();return application.createFieldServiceApplication({actorId:a.userId,actorUserIds:[],canRead:true,canReview:false,clock:{now:()=>{event('fake.clock',now);return now}},authorize:async write=>{event('fake.applicationAuthorize',!!write)},authorizeReview:async()=>{},replay:async(key,op,id,input)=>{event('fake.applicationReplay',key,op,id,input,{reason:typeof input.reason==='string'?utf16(input.reason):null,reservationId:typeof input.reservationId==='string'?utf16(input.reservationId):null});return null},order:async id=>{event('fake.order',id,utf16(id));return options.noOrder?null:{...row,id,status:options.action==='reserve-part'?'scheduled':'in_visit'}},reservation:async(id,rid)=>{event('fake.reservation',id,rid);return {id:rid,quantity_milli:3000,consumed_milli:0,released_milli:0,warehouse_id:'warehouse',product_id:'product'}},prepareConsumption:async(_row,res,input,key,t)=>{event('fake.quote',res.id,input,key,t);if(options.noQuote)throw new shared.BusinessRuleError(422,'MANUAL_RATE_REQUIRED','Synthetic quote unavailable.');return {effect:{fake:true},id:'fake-consumption',valueMinor:200,journalId:'fake-journal'}},prepareBilling:async()=>{throw Error('OUT_OF_SCOPE_BILLING')},commit:async(m,result)=>{event('fake.commitPreparation',m.operation,m.id,m.input,result);event('workflow.executeCommand');return w.executeCommand(a,m.key,m.operation,m.input,m.id,result.version,result,[])},workspace:async()=>{throw Error('OUT_OF_SCOPE_READ')},agreement:async()=>null,detail:async()=>{throw Error('OUT_OF_SCOPE_DETAIL')}})}
// Exact source leaves; the fixed source SHA protects both extracted spans.
const ownerText=files[src.owner], invokeText=ownerText.slice(ownerText.indexOf('async function invoke<T>'),ownerText.indexOf('export const fieldServiceWorkspace=')), entryLine=ownerText.split('\n').find(x=>x.startsWith('export const changeServiceOrder='));
if(!invokeText.startsWith('async function invoke<T>')||!entryLine||!entryLine.includes('withServicePurchasedCommand'))throw Error('OWNER_EXTRACTION_DRIFT');
globalThis.__fieldServiceOracleApplication=syntheticApplication;
const owner=mod(src.owner,{'@erpx/server-shared/index':shared,'@erpx/server-runtime/public/http':http,'@erpx/server-field_service/adapters/purchases':purchased,'@erpx/server-field_service/domain/service':domain},`import {BusinessRuleError} from '@erpx/server-shared/index';\nimport {ApiError} from '@erpx/server-runtime/public/http';\nimport {withServicePurchasedCommand} from '@erpx/server-field_service/adapters/purchases';\nimport {orderActionSchema} from '@erpx/server-field_service/domain/service';\nconst application=globalThis.__fieldServiceOracleApplication;\n${invokeText}\n${entryLine}`);
function spy(name){return async(_a,...args)=>{event('fake.dispatch',name,...args);return {fakeDispatch:name}}}
const route=mod(src.route,{'@erpx/feature-work_projection/lib/destination-links':{canonicalDestinationLinks:x=>x},'@erpx/server-identity_access/public/authorization':{requireWorkContext:async slug=>{event('fake.requireWorkContext',slug);if(options.denyAuth)throw Error('UNAUTHENTICATED');return actor}},'@erpx/server-manage_erpx/public/public-origin':{requestPublicOrigin:r=>{event('actual.publicOrigin');return origin.publicOrigin({host:r.headers.get('host'),url:r.url},{frontOrigins:[],origin:'https://holding.example.test',production:true})}},'@erpx/server-runtime/public/http':http,'@erpx/server-field_service/public/field-service':{changeServiceOrder:async(a,id,input,key)=>{event('owner.changeServiceOrder',id,utf16(id));options.action=input.action;return owner.changeServiceOrder(a,id,input,key)},createServiceRequest:spy('request'),changeAgreement:spy('changeAgreement'),createAgreement:spy('createAgreement'),createServiceOrder:spy('createOrder'),fieldServiceDetail:spy('detail'),fieldServiceWorkspace:spy('workspace')}});
const encoded=x=>JSON.stringify(x);
const baseHeaders=[['host','holding.example.test'],['origin','https://holding.example.test'],['idempotency-key','valid-key-123'],['content-type','application/json']];

// Only mandatory fakes differ: read/write permission toggle; all real source bindings above retain their exact hashes.
if(require('pg/package.json').version!=='8.23.1')throw Error('PG_VERSION_DRIFT');
const pgUtils=require('pg/lib/utils.js');const serializer=require('pg-protocol').serialize;
const driverPaths=['node_modules/pg/package.json','node_modules/pg/lib/utils.js','node_modules/pg-protocol/package.json','node_modules/pg-protocol/dist/serializer.js','node_modules/pg-protocol/dist/buffer-writer.js'];
const driverHashes=Object.fromEntries(driverPaths.map(p=>[p,hash(readFileSync(p))]));
function bindParameter(value){const bytes=serializer.bind({values:[value],valueMapper:pgUtils.prepareValue});let i=5;while(bytes[i++]!==0){}while(bytes[i++]!==0){}const n=bytes.readInt16BE(i);i+=2+n*2;const count=bytes.readInt16BE(i);i+=2;if(count!==1)throw Error('BIND_SHAPE');const len=bytes.readInt32BE(i);i+=4;return bytes.subarray(i,i+len);}
const targetValues=[['empty',''],['long220','x'.repeat(220)],['NUL','a\u0000b'],['high','a\ud800b'],['low','a\udc00b'],['replacement','a\ufffdb'],['pair','a😀b'],['unicode','งาน😀']].map(([id,value])=>[id,value.replaceAll('\\u0000','\u0000').replaceAll('\\ud800','\ud800').replaceAll('\\udc00','\udc00').replaceAll('\\ufffd','\ufffd')]);
const fingerprints=[];
function workflowProbe(id,record){let digests=[];let saved=null;let events=[];const app=workflow.createCommandApplication({repository:{receipt:async key=>{events.push(['receipt',key]);return saved},clearAssertions:()=>({}),insertReceipt:(_a,key,op,digest,rid,result)=>{saved={request_hash:digest,result_json:JSON.stringify(result),operation:op,actor_id:actor.userId,record_id:rid};events.push(['insert',key]);return{}},captureAssignmentAuthority:()=>({}),audit:()=>({}),outbox:()=>({})},clock:{now:()=>now},identifiers:{next:()=> 'id'},digest:async v=>{const encoded=JSON.stringify(v),digest=hash(encoded);digests.push({encoded,digest,recordUtf16:utf16(v.recordId)});return digest},workContextGuards:()=>[],iamMutationGuards:()=>[],assignmentGuard:()=>({}),prepareOwnerStatements:async()=>[],batch:async()=>{},activeContext:async()=>null,activeGrant:async()=>null});return {app,digests,events};}
for(const[id,value]of targetValues){const p=workflowProbe(id,value);const input=domain.orderActionSchema.parse(reserve);const result={id:value,version:8,status:'scheduled'};await p.app.executeCommand(actor,'valid-key-123','field.order.reserve-part',input,value,8,result,[]);const before=p.events.length;const replay=await p.app.replayCommand(actor,'bad!','field.order.reserve-part',value,input);let lateError;try{await p.app.executeCommand(actor,'bad!','field.order.reserve-part',input,value,8,result,[])}catch(e){lateError={name:e.name,status:e.status,code:e.code,message:e.message}}
 const bound=bindParameter(value);fingerprints.push({id,originalUtf16:utf16(value),json:JSON.stringify(value),bindHex:bound.toString('hex'),bindBytes:bound.length,physicalUtf8:bound.toString('utf8'),physicalUtf16:utf16(bound.toString('utf8')),fingerprint:p.digests[0],invalidNonemptyKeyExactReplay:{value:replay,events:p.events.slice(before)},executeInvalidNonemptyKeyError:lateError});}
const routes=[];
for(const[id,body,opts]of[['valid-denied-write',issue,{denyWrite:true}],['valid-denied-read',issue,{denyRead:true}],['schema-denied-read',{...issue,quantityMilli:0},{denyRead:true}],['schema-denied-write',{...issue,quantityMilli:0},{denyWrite:true}],['json-denied-write',null,{denyWrite:true,wire:'{'}],['auth-no-json',null,{denyAuth:true,wire:'{'}]]){trace=[];options=opts;const req=new Request('https://holding.example.test/api/v1/holdings/holding/field-service',{method:'POST',headers:baseHeaders,body:opts.wire??JSON.stringify(body)});const parse=req.json.bind(req);req.json=()=>{event('request.json');return parse()};const response=await route.POST(req,{params:Promise.resolve({holdingSlug:'holding'})});routes.push({id,options:opts,trace:structuredClone(trace),response:{status:response.status,headers:Object.fromEntries(response.headers),body:await response.text()}});}
const schema=[];for(const [id,body]of [['missing-number',{...issue,expectedVersion:undefined}],['wrong-number',{...issue,expectedVersion:'7'}],['missing-string',{...issue,reservationId:undefined}],['wrong-string',{...issue,reservationId:null}],['too-big-reason',{...issue,reason:'x'.repeat(3001)}],['too-big-allocations',{...issue,allocations:Array.from({length:201},()=>({}))}],['invalid-entry-date',{...issue,entryDate:'invalid'}],['invalid-action',{...issue,action:'other'}]]){try{domain.orderActionSchema.parse(body);throw Error('EXPECTED_ERROR')}catch(e){schema.push({id,first:e.issues?.[0]??null,http:{status:http.errorResponse(e).status,body:await http.errorResponse(e).text()}})}}
for(const[p,h]of Object.entries(expectedSources))if(hash(readFileSync(p))!==h)throw Error('SOURCE_AFTER:'+p);for(const[p,h]of Object.entries(driverHashes))if(hash(readFileSync(p))!==h)throw Error('DRIVER_AFTER:'+p);
const out={profile:{TZ:process.env.TZ,LANG:process.env.LANG,LC_ALL:process.env.LC_ALL,locale:Intl.DateTimeFormat().resolvedOptions().locale,timeZone:Intl.DateTimeFormat().resolvedOptions().timeZone},acceptance:false,scope:'actual retained source/fingerprint/replay/Headers/schema with fake authority/receipt/owner data and actual local pg serializer; NO PG',baseGeneratorSha256:'5151c725fcba0d023c049ec2bc982ffd9b2568d52d8028e41e82968597d71518',sourceHashes:expectedSources,driverHashes,runtime:{node:process.version,v8:process.versions.v8,icu:process.versions.icu,locale:Intl.DateTimeFormat().resolvedOptions().locale,timeZone:Intl.DateTimeFormat().resolvedOptions().timeZone,typescriptWrapper:wrapper.version,typescriptEngine:ts.version,pg:require('pg/package.json').version},targets:fingerprints,routes,schema};const bytes=JSON.stringify(out,null,2)+'\n';const target=resolve(fixtureRoot,'source-probes.v1.json');if(process.argv.includes('--check')){if(readFileSync(target,'utf8')!==bytes)throw Error('PROBE_DRIFT')}else writeFileSync(target,bytes,{mode:0o600});process.stdout.write(JSON.stringify({check:process.argv.includes('--check'),sha256:hash(bytes),targets:fingerprints.length,routes:routes.length,schema:schema.length})+'\n');
