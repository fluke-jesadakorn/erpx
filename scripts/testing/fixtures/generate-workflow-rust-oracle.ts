/** Execute unchanged B0 command and terminology rules on the pinned Node runtime. */
import {createHash} from "node:crypto";
import {execFileSync} from "node:child_process";
import {mkdir,readFile,writeFile} from "node:fs/promises";
import {resolve} from "node:path";

const sourceCommit="a5ed8737a2f05eff1ee9841fda4a094b3926d732";
const paths=["packages/server-workflow/src/application/commands.ts","packages/server-workflow/src/domain/commands.ts","packages/server-workflow/src/adapters/commands.ts","packages/feature-identity_access/src/lib/guest-account-compat.ts","packages/server-shared/src/index.ts","packages/server-shared/src/domain/error.ts"];
const sources=[];
for(const path of paths){const bytes=await readFile(path);if(!bytes.equals(execFileSync("git",["show",`${sourceCommit}:${path}`])))throw new Error(`WORKFLOW_ORACLE_SOURCE_CHANGED:${path}`);sources.push({path,sha256:createHash("sha256").update(bytes).digest("hex")});}
const node=process.env.ERPX_NODE_BIN??"node";
if(execFileSync(node,["--version"],{encoding:"utf8"}).trim()!=="v26.11.1")throw new Error("PINNED_WORKFLOW_ORACLE_NODE_REQUIRED");
const directory=resolve(".scratch/dev/workflow-rust-oracle");await mkdir(directory,{recursive:true,mode:0o700});
const entry=resolve(directory,"entry.ts");
await writeFile(entry,`
import {createCommandApplication} from ${JSON.stringify(resolve(paths[0]!))};
import {canonicalGuestAccount,legacyGuestAccount,guestAccountOperation,legacyGuestAccountOperation} from ${JSON.stringify(resolve(paths[3]!))};
import {readFileSync} from "node:fs";
import {createHash} from "node:crypto";
const input=JSON.parse(readFileSync(0,"utf8"));
function errorResult(error){return {code:typeof error.code==="string"?error.code:null,status:typeof error.status==="number"?error.status:null,message:error.message};}
function application(options={}){
 const calls={fingerprints:[],guards:[],batches:[],writes:[],prepared:0};let receipt=options.receipt??null;let sequence=0;
 const effect=(kind,...args)=>({kind,args});
 const app=createCommandApplication({
  repository:{receipt:async()=>receipt,clearAssertions:id=>effect("clear",id),insertReceipt:(actor,id,operation,digest,recordId,result,now)=>{const row={request_hash:digest,result_json:JSON.stringify(result),operation,actor_id:actor.userId,record_id:recordId,authority_json:options.savedAuthorityJson??null};calls.writes.push(row);return effect("receipt",id);},captureAssignmentAuthority:()=>effect("capture"),audit:()=>effect("audit"),outbox:()=>effect("outbox")},
  clock:{now:()=>1800000000000},identifiers:{next:()=>"synthetic-event-"+(++sequence)},
  digest:async value=>{const expectedJson=JSON.stringify(value),expectedHash=createHash("sha256").update(expectedJson).digest("hex");calls.fingerprints.push({expectedJson,expectedHash});return expectedHash;},
  workContextGuards:(actor,id)=>{calls.guards.push({kind:"context",actor,id});return [effect("context")];},
  iamMutationGuards:(actor,id)=>{calls.guards.push({kind:"iam",actor,id});return [effect("iam")];},assignmentGuard:()=>effect("assignment"),
  prepareOwnerStatements:async(_actor,_id,_operation,_input,_record,_version,effects)=>{calls.prepared++;return effects;},
  batch:async effects=>{calls.batches.push(effects.map(row=>row.kind));if(options.guardState)throw Object.assign(new Error("Synthetic current authority rejection"),{code:options.guardState});return [];},
  activeContext:async()=>null,activeGrant:async()=>null,
 });return {app,calls};
}
async function fingerprint(row){
 const f=application();const actor=JSON.parse(row.actorJson),value=JSON.parse(row.inputJson);
 try{await f.app.executeCommand(actor,"synthetic-fingerprint-key",row.operation,value,row.recordId,1,{ok:true},[]);return {...row,actor,...f.calls.fingerprints[0],expectedError:null};}
 catch(error){return {...row,actor,expectedJson:f.calls.fingerprints[0]?.expectedJson??null,expectedHash:f.calls.fingerprints[0]?.expectedHash??null,expectedError:errorResult(error)};}
}
const json=input.json.map(inputJson=>({inputJson,expectedJson:JSON.stringify(JSON.parse(inputJson))}));
const fingerprints=[];for(const row of input.fingerprints)fingerprints.push(await fingerprint(row));
const compatibility=input.compatibility.map(inputJson=>{const row={inputJson};for(const [name,convert] of [["canonical",canonicalGuestAccount],["legacy",legacyGuestAccount]]){try{row[name+"Json"]=JSON.stringify(convert(JSON.parse(inputJson)));row[name+"Error"]=null;}catch(error){row[name+"Json"]=null;row[name+"Error"]=error.message;}}return row;});
const operations=input.operations.map(operation=>({operation,canonical:guestAccountOperation(operation),legacy:legacyGuestAccountOperation(operation)}));
const replays=[];
for(const row of input.replays){
 const original=application({savedAuthorityJson:row.savedAuthorityJson});
 await original.app.executeCommand(JSON.parse(row.receiptActorJson),"synthetic-replay-key",row.receiptOperation,JSON.parse(row.receiptInputJson),row.receiptRecordId??row.recordId,1,JSON.parse(row.receiptResultJson),[]);
 const receipt={...original.calls.writes[0],...row.receiptOverrides};
 const f=application({receipt,guardState:row.guardState});let expectedResultJson=null,expectedError=null;
 try{expectedResultJson=JSON.stringify(await f.app.replayCommand(JSON.parse(row.actorJson),"synthetic-replay-key",row.operation,row.recordId,JSON.parse(row.inputJson)));}catch(error){expectedError=errorResult(error);}
 replays.push({...row,actor:JSON.parse(row.actorJson),receipt,expectedResultJson,expectedError,currentGuardActors:f.calls.guards.map(row=>({kind:row.kind,actor:row.actor})),batchKinds:f.calls.batches,writes:f.calls.writes.length});
}
process.stdout.write(JSON.stringify({node:process.version,json,fingerprints,compatibility,operations,replays}));
`,{mode:0o600});
const built=await Bun.build({entrypoints:[entry],outdir:directory,target:"node",format:"esm",naming:"oracle.mjs"});
if(!built.success)throw new Error("WORKFLOW_NODE_ORACLE_BUILD_FAILED");

const json=["null","true","false","0","-0","-0.0","1.0","1e0","1e+21","1e20","1e-6","1e-7","1e-400","-1e-400","5e-324","2.2250738585072014e-308","1.7976931348623157e308","1e400","-1e400","9007199254740993","18446744073709551615","0.10000000000000000555","333333333.33333329",'"Thai ก accent é e\\u0301"','"\\ud83d\\ude00"','"\\ud800"','"\\udc00"','"a\\ud800b"','"\\ud800\\ud800"','"\\udc00\\ud800"','"\\u0000\\b\\t\\n\\f\\r\\\"\\\\"','"line\\u2028paragraph\\u2029end"',"[]","{}",'[0,-0,1.0,1e400,null,false]',
 '{"z":0,"2":"two","1":"one","a":1,"0":"zero"}',
 '{"4294967295":"ordinary","4294967294":"index","01":"ordinary","-0":"ordinary","1.0":"ordinary","1":"index","+1":"ordinary"}',
 '{"b":1,"a":2,"b":3}', '{"2":1,"1":2,"2":3,"a":4}',
 '{"é":1,"e\\u0301":2,"ก":3,"\\ud800":4}',
 '{"outer":{"z":-0,"2":1.0,"1":1e-7,"z":1e21},"a":[{"b":1,"a":2},null]}',
 '{"constructor":1,"__proto__":{},"toString":2,"toJSON":3}',
 '{"id":"guest-account","amount":"100.00","version":9007199254740991}',
 '{"0":0,"00":1,"000":2,"-1":3,"1e0":4,"1":5,"10":6,"2":7}',
 '{"a":{"x":1,"x":2},"a":{"y":3},"b":4}',
 '{"1":1,"\\u0031":2,"a":3}',
 '["NFC é","NFD e\\u0301","กํา","กำ","😀","\\ud800"]'];
const base={userId:"synthetic-user",assignmentId:"synthetic-assignment",role:"pms_cashier",companyId:"synthetic-company"};
const authority={sessionId:"synthetic-session",revision:2,assignmentVersion:3};
const actors=[base,{...base,branchId:null,departmentId:null},{...base,branchId:"synthetic-branch"},{...base,branchId:null,departmentId:"synthetic-department"},{...base,branchId:"synthetic-branch",departmentId:"synthetic-department"},{...base,authority:null},{...base,authority},{...base,authority,branchId:null,departmentId:null},{...base,authority:{...authority,revision:9_007_199_254_740_991}},{...base,iam:null},{...base,iam:{queuedAssignmentVersion:3,identityVersion:7,roleRevision:1}},{...base,authority,iam:{queuedAssignmentVersion:3,identityVersion:7,roleRevision:1,hrAppointmentVersion:null}},{...base,role:"custom_พนักงาน_é",authority,branchId:null,departmentId:null}];
const fingerprints=json.flatMap((inputJson,index)=>[actors[0]!,actors[1]!,actors[6]!,actors[7]!].map((actor,actorIndex)=>({id:`json-${index}-actor-${actorIndex}`,actorJson:JSON.stringify(actor),operation:"record.update",recordId:"synthetic-record",inputJson})));
for(const [index,actor] of actors.entries())for(const operation of ["record.update","pms.guest_account.close","stay.route_guest_account"])fingerprints.push({id:`actor-${index}-${operation}`,actorJson:JSON.stringify(actor),operation,recordId:"record-é-ก",inputJson:'{"guestAccountId":"record-é-ก","amount":"100.00","version":3}'});
const compatibility=["null","false","3",'"folio"','[]','{}',
 '{"folio":"x","folios":["a"],"folioId":"id","toFolioId":"to","folioIds":["id"],"folio_id":"id","folio_entry_id":"entry","folioEntryId":"entry"}',
 '{"guestAccount":"x","guestAccounts":["a"],"guestAccountId":"id","toGuestAccountId":"to","guestAccountIds":["id"],"guest_account_id":"id","guest_account_entry_id":"entry","guestAccountEntryId":"entry"}',
 '{"folioId":"same","guestAccountId":"same"}', '{"guestAccountId":"same","folioId":"same"}', '{"folioId":"one","guestAccountId":"two"}', '{"guestAccountId":"one","folioId":"two"}',
 '{"folio":{"a":1,"b":2},"guestAccount":{"a":1,"b":2}}', '{"folio":{"a":1,"b":2},"guestAccount":{"b":2,"a":1}}',
 '{"operation":"pms.folio.close","eventType":"stay.route_folio","kind":"folio","recordKind":"folio","section":"folios","nested":[{"folioId":"nested"}]}',
 '{"operation":"pms.guest_account.close","eventType":"stay.route_guest_account","kind":"guest_account","recordKind":"guest_account","section":"guest-accounts","nested":[{"guestAccountId":"nested"}]}',
 '{"kind":"other","recordKind":"other","section":"other","operation":"other.folio.close","eventType":null}',
 '{"folios":[{"folioId":"a"}],"guestAccounts":[{"guestAccountId":"a"}]}',
 '{"folioId":-0,"guestAccountId":0}', '{"folioId":1e400,"guestAccountId":null}',
 '{"folioId":"é","guestAccountId":"e\\u0301"}', '{"nested":{"folioId":"one","guestAccountId":"two"}}',
 '{"0":{"folioId":"zero"},"2":{"guestAccountId":"two"},"1":{"folioId":"one"}}',
 '{"folioId":"\\ud800","guestAccountId":"\\ud800"}',
 ...["__proto__","constructor","toString","toLocaleString","valueOf","hasOwnProperty","isPrototypeOf","propertyIsEnumerable","__defineGetter__","__defineSetter__","__lookupGetter__","__lookupSetter__","toJSON"].flatMap(key=>[JSON.stringify({[key]:{}}),JSON.stringify({[key]:"synthetic"})]),
 '{"__proto__":{},"[object Object]":{}}', '{"__proto__":{},"[object Object]":{"different":true}}',
 '{"constructor":1,"function Object() { [native code] }":1}', '{"constructor":1,"function Object() { [native code] }":2}'];
const operations=["folio.post","guest_account.post","pms.folio.close","pms.guest_account.close","stay.route_folio","stay.route_guest_account","pms.stay.route_folio","prefix.stay.route_folio.tail","folio","guest_account","pms.folio","other.folio.close","pms.guest_accounting.close","stay.route_folio.stay.route_folio","stay.route_guest_account.stay.route_guest_account"];

const current={...base,branchId:null,departmentId:null,authority};
const original={...current,authority:{...authority,revision:1}};
const saved={role:base.role,assignmentId:base.assignmentId,...original.authority,companyId:base.companyId,branchId:null,departmentId:null};
const replayBase={actorJson:JSON.stringify(current),receiptActorJson:JSON.stringify(original),operation:"pms.guest_account.close",receiptOperation:"pms.folio.close",recordId:"synthetic-record",receiptRecordId:"synthetic-record",inputJson:'{"guestAccountId":"synthetic-record"}',receiptInputJson:'{"folioId":"synthetic-record"}',receiptResultJson:'{"folioId":"synthetic-record","operation":"pms.folio.close","nested":[{"kind":"folio"}]}',savedAuthorityJson:JSON.stringify(saved),receiptOverrides:{} as Record<string,unknown>,guardState:null as string|null};
const replays=[
 {id:"saved-authority-reconstruction",...replayBase},
 {id:"reconstruction-current-authority-denial",...replayBase,guardState:"23514"},
 {id:"reconstruction-other-current-guard-error",...replayBase,guardState:"23503"},
 {id:"changed-current-company",...replayBase,actorJson:JSON.stringify({...current,companyId:"other-company"})},
 {id:"changed-current-role",...replayBase,actorJson:JSON.stringify({...current,role:"pms_manager"})},
 {id:"changed-current-assignment",...replayBase,actorJson:JSON.stringify({...current,assignmentId:"other-assignment"})},
 {id:"changed-current-branch",...replayBase,actorJson:JSON.stringify({...current,branchId:"other-branch"})},
 {id:"changed-current-department",...replayBase,actorJson:JSON.stringify({...current,departmentId:"other-department"})},
 {id:"changed-input",...replayBase,inputJson:'{"guestAccountId":"different-record"}'},
 {id:"changed-record",...replayBase,recordId:"other-record"},
 {id:"saved-authority-field-type-mismatch",...replayBase,savedAuthorityJson:JSON.stringify({...saved,revision:"1"})},
 {id:"saved-null-vs-omitted-branch",...replayBase,actorJson:JSON.stringify({...base,authority})},
 {id:"saved-authority-missing",...replayBase,savedAuthorityJson:null},
 {id:"stored-actor-mismatch",...replayBase,receiptOverrides:{actor_id:"other-user"}},
 {id:"stored-record-mismatch",...replayBase,receiptOverrides:{record_id:"other-record"}},
 {id:"canonical-operation-cannot-reconstruct",...replayBase,receiptOverrides:{operation:"pms.guest_account.close"}},
 {id:"same-current-authority-canonical-receipt",...replayBase,receiptActorJson:JSON.stringify(current),receiptOperation:"pms.guest_account.close",receiptInputJson:replayBase.inputJson},
 {id:"same-current-authority-legacy-receipt",...replayBase,receiptActorJson:JSON.stringify(current)},
 {id:"same-current-authority-denial",...replayBase,receiptActorJson:JSON.stringify(current),guardState:"23514"},
 {id:"unversioned-legacy-receipt",...replayBase,actorJson:JSON.stringify(base),receiptActorJson:JSON.stringify(base),savedAuthorityJson:null},
 {id:"queued-metadata-requires-current-guards",...replayBase,actorJson:JSON.stringify({...base,iam:{queuedAssignmentVersion:3}}),receiptActorJson:JSON.stringify(base),savedAuthorityJson:null,guardState:"23514"},
 {id:"null-authority-legacy-receipt",...replayBase,actorJson:JSON.stringify({...base,authority:null}),receiptActorJson:JSON.stringify({...base,authority:null}),savedAuthorityJson:null},
 {id:"alias-input-conflict",...replayBase,inputJson:'{"guestAccountId":"one","folioId":"two"}'},
 {id:"alias-result-conflict",...replayBase,receiptResultJson:'{"guestAccountId":"one","folioId":"two"}'},
];
const output=JSON.parse(execFileSync(node,[resolve(directory,"oracle.mjs")],{input:JSON.stringify({json,fingerprints,compatibility,operations,replays}),encoding:"utf8",maxBuffer:24*1024*1024,env:{PATH:process.env.PATH??"",TZ:"UTC",LANG:"en_US.UTF-8",NODE_ENV:"test"}}));
const document=JSON.stringify({version:1,sourceCommit,sources,node:output.node,json:output.json,fingerprints:output.fingerprints,compatibility:output.compatibility,operations:output.operations,replays:output.replays},null,2)+"\n";
const destination="fixtures/workflow/rust-commands/conformance.v1.json";
if(process.argv.includes("--check")){if(await readFile(destination,"utf8")!==document)throw new Error("WORKFLOW_RUST_ORACLE_DRIFT");}
else{await mkdir(resolve(destination,".."),{recursive:true});await writeFile(destination,document);}
console.log(`Workflow Node oracle: ${output.json.length} JSON lexical cases, ${output.fingerprints.length} actual fingerprints, ${output.compatibility.length} value mappings, ${output.operations.length} operations and ${output.replays.length} replay cases${process.argv.includes("--check")?" (unchanged)":""}.`);
