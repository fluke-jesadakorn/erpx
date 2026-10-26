/** Freeze decisions from unchanged B0 rules executed on pinned Node. */
import {createHash} from "node:crypto";
import {execFileSync} from "node:child_process";
import {mkdir,readFile,writeFile} from "node:fs/promises";
import {resolve} from "node:path";
import {commercialPackages,type CompanyPackageGrant,type CompanyPackageHistoryRight} from "../../../packages/server-commercial/src/domain/packages";

const sourceCommit="a5ed8737a2f05eff1ee9841fda4a094b3926d732",sourcePaths=["packages/server-commercial/src/domain/packages.ts","packages/contracts/src/product-registry.ts"];
const sources=[];
for(const path of sourcePaths){const bytes=await readFile(path);if(!bytes.equals(execFileSync("git",["show",`${sourceCommit}:${path}`])))throw new Error("COMMERCIAL_ORACLE_SOURCE_CHANGED");sources.push({path,sha256:createHash("sha256").update(bytes).digest("hex")});}
const commandPaths=["packages/server-commercial/src/application/package-grants.ts","packages/server-commercial/src/domain/package-dependencies.ts"];
const commandSources=[...sources];
for(const path of commandPaths){const bytes=await readFile(path);if(!bytes.equals(execFileSync("git",["show",`${sourceCommit}:${path}`])))throw new Error("COMMERCIAL_COMMAND_ORACLE_SOURCE_CHANGED");commandSources.push({path,sha256:createHash("sha256").update(bytes).digest("hex")});}
const batchPaths=["packages/server-commercial/src/application/package-grant-batches.ts","packages/server-commercial/src/adapters/package-grants.ts"];
const batchSources=[...commandSources];
for(const path of batchPaths){const bytes=await readFile(path);if(!bytes.equals(execFileSync("git",["show",`${sourceCommit}:${path}`])))throw new Error("COMMERCIAL_BATCH_ORACLE_SOURCE_CHANGED");batchSources.push({path,sha256:createHash("sha256").update(bytes).digest("hex")});}
const revisionSource=(await readFile(batchPaths[1]!,"utf8")).slice((await readFile(batchPaths[1]!,"utf8")).indexOf("export function packageGrantRevision("));
if(!revisionSource.startsWith("export function packageGrantRevision("))throw new Error("COMMERCIAL_BATCH_REVISION_SOURCE_REQUIRED");
const node=process.env.ERPX_NODE_BIN??"node";if(execFileSync(node,["--version"],{encoding:"utf8"}).trim()!=="v26.11.1")throw new Error("PINNED_COMMERCIAL_ORACLE_NODE_REQUIRED");
const directory=resolve(".scratch/dev/commercial-rust-oracle");await mkdir(directory,{recursive:true,mode:0o700});
const entry=resolve(directory,"entry.ts");
await writeFile(entry,`
import {commercialPackages,canonicalPackageGrant,purchasedCapability,requiresPackageSettlementReview,validateGrantChange} from ${JSON.stringify(resolve(sourcePaths[0]!))};
import {readFileSync} from "node:fs";
import {createHash} from "node:crypto";
import {packageGrantCommands} from ${JSON.stringify(resolve(commandPaths[0]!))};
import {validatePackageDependencies} from ${JSON.stringify(resolve(commandPaths[1]!))};
import {packageGrantBatchCommands} from ${JSON.stringify(resolve(batchPaths[0]!))};
${revisionSource}
const input=JSON.parse(readFileSync(0,"utf8"));
const collator=new Intl.Collator();
const runtime={icu:process.versions.icu,cldr:process.versions.cldr,unicode:process.versions.unicode,collator:collator.resolvedOptions()};
if(runtime.icu!=="78.3"||runtime.cldr!=="48.0"||runtime.unicode!=="17.0"||runtime.collator.locale!=="en-US"||runtime.collator.usage!=="sort"||runtime.collator.sensitivity!=="variant"||runtime.collator.ignorePunctuation!==false||runtime.collator.collation!=="default"||runtime.collator.numeric!==false||runtime.collator.caseFirst!=="false")throw new Error("PINNED_COMMERCIAL_UNICODE_RUNTIME_REQUIRED");
async function batchHash(row){
 let expectedHash=null,normalized=null,expectedCode=null;
 const commands=packageGrantBatchCommands({unit:()=>{throw new Error("ORACLE_STOP");},transaction:async()=>{throw new Error("ORACLE_STOP");},hash:value=>{normalized=JSON.parse(value);return expectedHash=createHash("sha256").update(value).digest("hex");}});
 try{await commands.apply(row,async()=>{});}catch(error){if(error.message!=="ORACLE_STOP")expectedCode=error.message;}
 return {input:row,expectedHash,normalized,expectedCode};
}
process.stdout.write(JSON.stringify({node:process.version,runtime,packages:commercialPackages,
batchHashes:await Promise.all(input.batches.map(batchHash)),
unicodeHashes:await Promise.all(input.unicodeBatches.map(batchHash)),
unicodeRevisions:input.unicodeScenarios.map(row=>({grants:row,expectedRevision:packageGrantRevision("company",row)})),
unicodeComparisons:input.unicodeIds.flatMap(left=>input.unicodeIds.map(right=>({left,right,expectedOrdering:Math.sign(left.localeCompare(right))}))),
unicodeProspectives:await Promise.all(input.unicodeProspectives.map(async row=>{
 const request={...row.input,expectedRevision:packageGrantRevision("company",row.before)};
 const grants={authorize:async()=>({userId:"synthetic"}),lock:async()=>{},current:async(companyId,packageId)=>row.before.find(grant=>grant.companyId===companyId&&grant.packageId===packageId)??null,receipt:async()=>null,obligations:async()=>0,dependencies:async()=>{},save:async()=>{throw new Error("PREVIEW_CANNOT_WRITE");}};
 const unit={grants,all:async()=>row.before,revision:packageGrantRevision,previewDependencies:async()=>{},receipt:async()=>null,saveReceipt:async()=>{throw new Error("PREVIEW_CANNOT_WRITE");}};
 const commands=packageGrantBatchCommands({unit:()=>unit,transaction:async()=>{throw new Error("PREVIEW_CANNOT_WRITE");},hash:value=>createHash("sha256").update(value).digest("hex")});
 return {input:request,before:row.before,expectedPreview:await commands.preview(request)};
})),
revisions:input.scenarios.map(row=>({grants:row.grants,expectedRevision:packageGrantRevision("company",row.grants)})),
decisions:input.decisions.map(row=>({...row,expected:purchasedCapability(input.scenarios[row.scenario].grants,"company",row.capability,row.purpose,5000,input.scenarios[row.scenario].history)})),
hashes:await Promise.all(input.changes.map(async row=>{
 let requestHash=null;const unit={authorize:async()=>({userId:"synthetic"}),lock:async()=>{},current:async()=>row.current,receipt:async()=>null,obligations:async()=>row.outstanding,dependencies:async()=>{},save:async()=>{}};
 const commands=packageGrantCommands({unit:()=>unit,transaction:async action=>action(unit),hash:value=>(requestHash=createHash("sha256").update(value).digest("hex"))});
 try{await commands.apply({grant:row.next,expectedVersion:row.expectedVersion,idempotencyKey:"native-grant-fixture-key"});}catch{}
 return {input:{grant:row.next,expectedVersion:row.expectedVersion,idempotencyKey:"native-grant-fixture-key"},expectedHash:requestHash};
})),
dependencies:input.dependencies.map(row=>{let expectedCode=null;try{validatePackageDependencies(row.grant,input.scenarios[row.scenario].grants,5000);}catch(error){expectedCode=error.message;}return {...row,expectedCode};}),
changes:input.changes.map(row=>{let expectedCode=null;try{validateGrantChange(row.current,row.next,row.expectedVersion,row.outstanding);}catch(error){expectedCode=error.message;}return {...row,expectedCode,requiresReview:requiresPackageSettlementReview(row.current,row.next)};})}));
`,{mode:0o600});
const build=await Bun.build({entrypoints:[entry],outdir:directory,target:"node",format:"esm",naming:"oracle.mjs"});if(!build.success)throw new Error("COMMERCIAL_NODE_ORACLE_BUILD_FAILED");
const grant=(packageId:string,extra:Partial<CompanyPackageGrant>={}):CompanyPackageGrant=>({companyId:"company",packageId,packageVersion:1,state:"active",startsAt:0,endsAt:null,version:1,reason:"Synthetic approved period",evidence:"Synthetic receipt",...extra});
const right=(packageId:string,extra:Partial<CompanyPackageHistoryRight>={}):CompanyPackageHistoryRight=>({companyId:"company",packageId,packageVersion:1,sourceGrantVersion:1,sourceEventId:"synthetic-prior-event",retainedAt:1000,...extra});
const scenarios:{grants:CompanyPackageGrant[];history:CompanyPackageHistoryRight[]}[]=[];
function add(grants:CompanyPackageGrant[],history:CompanyPackageHistoryRight[]=[]){scenarios.push({grants,history});}
add([]);add(commercialPackages.map(p=>grant(p.id)));
for(const extra of [{},{state:"draining" as const},{state:"inactive" as const},{endsAt:5000},{endsAt:4999},{startsAt:5000},{startsAt:5001},{packageVersion:2},{companyId:"foreign"}])add([grant("legal",extra)]);
for(const extra of [{},{state:"draining" as const},{state:"inactive" as const},{endsAt:5000},{startsAt:5001}])add([grant("finance-advanced"),grant("finance",extra)]);
add([grant("finance-advanced")]);add([], [right("legal")]);add([grant("legal",{state:"inactive"})],[right("legal")]);
for(const extra of [{sourceGrantVersion:0},{sourceEventId:""},{retainedAt:5001},{retainedAt:-1},{companyId:"foreign"},{packageVersion:2}])add([],[right("legal",extra)]);
add([],[right("finance-advanced"),right("finance")]);add([],[right("finance-advanced")]);
const capabilities=[...new Set(commercialPackages.flatMap(p=>[...p.visibleCapabilities,...p.includedCapabilities])),"unknown.capability"];
const decisions=scenarios.flatMap((_,scenario)=>capabilities.flatMap(capability=>["new-work","settle-existing","history"].map(purpose=>({scenario,capability,purpose}))));
const changes:{current:CompanyPackageGrant|null;next:CompanyPackageGrant;expectedVersion:number;outstanding:number}[]=[];
function change(current:CompanyPackageGrant|null,next:CompanyPackageGrant,expectedVersion=1,outstanding=0){changes.push({current,next,expectedVersion,outstanding});}
change(null,grant("legal"),0);change(null,grant("legal",{state:"draining"}),0);
for(const extra of [{},{state:"draining" as const},{state:"inactive" as const},{startsAt:6000},{endsAt:5001},{packageVersion:2},{packageId:"unknown"},{state:"invalid" as CompanyPackageGrant["state"]},{companyId:""},{reason:"\ufeff"},{evidence:" "},{reason:"\u0085"},{startsAt:-1},{endsAt:0},{version:1}])change(grant("legal"),grant("legal",{version:2,...extra}));
change(grant("legal",{state:"draining"}),grant("legal",{state:"inactive",version:2}));change(grant("legal",{state:"draining"}),grant("legal",{state:"inactive",version:2}),1,1);
change(grant("legal"),grant("legal",{version:2,startsAt:1000}),1,1);change(grant("legal"),grant("legal",{version:2,companyId:"foreign"}));
const dependencyGrants=[grant("finance"),grant("finance",{state:"inactive"}),grant("finance",{startsAt:100}),grant("finance",{endsAt:6000}),grant("finance-advanced"),grant("finance-advanced",{endsAt:6000}),grant("finance-advanced",{startsAt:100}),grant("finance-advanced",{state:"draining"}),grant("unknown")];
const dependencies=scenarios.flatMap((_,scenario)=>dependencyGrants.map(grant=>({scenario,grant})));
const batches=Array.from({length:commercialPackages.length},(_,index)=>({companyId:"company",expectedRevision:"a".repeat(64),idempotencyKey:"synthetic-package-batch-key",changes:[...commercialPackages.slice(0,index+1)].reverse().map(p=>({grant:grant(p.id,{reason:"Synthetic \u0e01\u0e02\u0e04 \\ line\nquote \"",evidence:"Synthetic evidence \u2028"}),expectedVersion:0}))}));
const invalidBatches=[{...batches[0]!,changes:[]},{...batches[0]!,expectedRevision:"A".repeat(64)},{...batches[0]!,idempotencyKey:"short"},{...batches[0]!,changes:[batches[0]!.changes[0]!,batches[0]!.changes[0]!]},{...batches[0]!,companyId:"foreign"}];
batches.push(...invalidBatches);
// Unknown persisted identifiers remain valid revision inputs. Do not normalize
// these byte-distinct strings: JS stable sort preserves canonical-equal ties.
const unicodeIds=["finance","finance-advanced","legal","legacy_z","legacy_ä","legacy_a\u0308","legacy_A","legacy_a","legacy-a","legacy_a10","legacy_a2","legacy_ก","legacy_ข","legacy_กำ","legacy_กํา","legacy_é","legacy_e\u0301","legacy_ß","legacy_ss","legacy_ø","legacy_o","legacy_ǻ","legacy_a\u030a\u0301","legacy_Å","legacy_A\u030a","legacy_แ","legacy_เ","legacy_\u{1e6c0}","legacy_\u{1e6c1}"];
const unicodeOrders=[unicodeIds,[...unicodeIds].reverse(),[...unicodeIds.slice(8),...unicodeIds.slice(0,8)],
 ["legacy_ä","legacy_a\u0308","legacy_z","finance-advanced","finance"],
 ["legacy_a\u0308","legacy_ä","legacy_z","finance-advanced","finance"],
 ["legacy_é","legacy_e\u0301","legacy_ǻ","legacy_a\u030a\u0301","legal"],
 ["legacy_e\u0301","legacy_é","legacy_a\u030a\u0301","legacy_ǻ","legal"],
 ["legacy_กำ","legacy_กํา","legacy_แ","legacy_เ","legacy_ก"],
 ["legacy_กํา","legacy_กำ","legacy_เ","legacy_แ","legacy_ก"]];
const unicodeScenarios=unicodeOrders.map(ids=>ids.map((id,index)=>grant(id,{reason:`Synthetic Unicode row ${index}`,evidence:`Synthetic ${id}`})));
// A foreign row must be excluded before the revision sort, as in the source.
unicodeScenarios.push([grant("legacy_z"),grant("legacy_ä"),grant("foreign_é",{companyId:"foreign"})]);
const unicodeBatches=unicodeScenarios.slice(0,9).map(rows=>({companyId:"company",expectedRevision:"a".repeat(64),idempotencyKey:"synthetic-unicode-batch-key",changes:rows.map(row=>({expectedVersion:0,grant:row}))}));
// Execute the actual legacy Map/preview workflow with Unicode history and a
// valid known-package change. Replacing a row preserves its Map insertion slot;
// appending a new known row must preserve all earlier canonical-equal ties.
const unicodeProspectives=unicodeScenarios.slice(0,9).flatMap(rows=>[true,false].map(replacing=>{
 const before=rows.filter(row=>row.packageId!=="legal");if(replacing)before.splice(Math.min(2,before.length),0,grant("legal"));
 return {before,input:{companyId:"company",expectedRevision:"a".repeat(64),idempotencyKey:"synthetic-unicode-preview-key",changes:[{expectedVersion:replacing?1:0,grant:grant("legal",{version:replacing?2:1})}]}};
}));
const oracle=JSON.parse(execFileSync(node,[resolve(directory,"oracle.mjs")],{input:JSON.stringify({scenarios,decisions,changes,dependencies,batches,unicodeIds,unicodeScenarios,unicodeBatches,unicodeProspectives}),encoding:"utf8",maxBuffer:16*1024*1024}));
await mkdir("fixtures/commercial/rust-packages",{recursive:true});await writeFile("fixtures/commercial/rust-packages/conformance.v1.json",JSON.stringify({version:1,sourceCommit,sources,node:oracle.node,packages:oracle.packages,scenarios,decisions:oracle.decisions,changes:oracle.changes},null,2)+"\n");
console.log(`Commercial oracle: ${oracle.decisions.length} decisions and ${oracle.changes.length} grant changes.`);

await writeFile("fixtures/commercial/rust-packages/grant-commands.v1.json",JSON.stringify({version:1,sourceCommit,sources:commandSources,node:oracle.node,scenarios,hashes:oracle.hashes,dependencies:oracle.dependencies},null,2)+"\n");
console.log(`Commercial command oracle: ${oracle.hashes.length} actual application hashes and ${oracle.dependencies.length} dependency decisions.`);

await writeFile("fixtures/commercial/rust-packages/grant-batches.v1.json",JSON.stringify({version:1,sourceCommit,sources:batchSources,node:oracle.node,hashes:oracle.batchHashes,revisions:oracle.revisions},null,2)+"\n");
console.log(`Commercial batch oracle: ${oracle.batchHashes.length} normalized requests and ${oracle.revisions.length} source revisions.`);

await writeFile("fixtures/commercial/rust-packages/grant-batches-unicode.v1.json",JSON.stringify({version:1,sourceCommit,sources:batchSources,node:oracle.node,runtime:oracle.runtime,comparisons:oracle.unicodeComparisons,hashes:oracle.unicodeHashes,revisions:oracle.unicodeRevisions,prospectives:oracle.unicodeProspectives},null,2)+"\n");
console.log(`Commercial Unicode oracle: ${oracle.unicodeComparisons.length} comparisons, ${oracle.unicodeHashes.length} normalized hashes, ${oracle.unicodeRevisions.length} revisions and ${oracle.unicodeProspectives.length} actual Map previews.`);
