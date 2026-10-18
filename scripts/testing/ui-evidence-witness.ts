import {constants} from "node:fs";
import {chmod,copyFile,lstat,readFile,realpath} from "node:fs/promises";
import {createHash} from "node:crypto";
import {basename,dirname,isAbsolute,join,relative,resolve} from "node:path";

export type UiEvidenceFile={path:string;sha256:string;bytes:number};
export type UiBrowserEvidenceWitness={schema:"erpx-ui-original-browser-leaves/v1";version:1;run:string;report:UiEvidenceFile;state:UiEvidenceFile;observations:UiEvidenceFile[];screenshots:UiEvidenceFile[];productHostLease?:UiEvidenceFile};
type Attachment={name?:unknown;contentType?:unknown;path?:unknown};
type Capture={binding:UiEvidenceFile;bytes:Buffer};
const hash=(bytes:Buffer)=>createHash("sha256").update(bytes).digest("hex");
const record=(value:unknown):value is Record<string,unknown>=>value!==null&&typeof value==="object"&&!Array.isArray(value);
function attachments(value:unknown):Attachment[]{
 const rows:Attachment[]=[];
 function walk(value:unknown){
  if(!record(value))return;
  if(Array.isArray(value.attachments))for(const attachment of value.attachments)if(record(attachment))rows.push(attachment);
  for(const key of ["suites","specs","tests","results"]){const children=value[key];if(Array.isArray(children))for(const child of children)walk(child);}
 }
 walk(value);return rows;
}
function internal(root:string,path:string){
 if(!path||path.includes("\\")||(!isAbsolute(path)&&path.split("/").some(part=>!part||part==="."||part==="..")))throw Error("UI_NATIVE_LEAF_PATH_REQUIRED");
 const file=resolve(root,path);if(!file.startsWith(root+"/"))throw Error("UI_NATIVE_LEAF_PATH_REQUIRED");return file;
}
async function capture(root:string,path:string,within:string):Promise<Capture>{
 const file=internal(root,path);if(!file.startsWith(within+"/"))throw Error("UI_NATIVE_LEAF_OUTSIDE_CASE");
 const before=await lstat(file,{bigint:true});if(!before.isFile()||await realpath(file)!==file||before.size>BigInt(64*1024*1024))throw Error("UI_NATIVE_REGULAR_BOUNDED_LEAF_REQUIRED");
 const bytes=await readFile(file),after=await lstat(file,{bigint:true});
 if(!after.isFile()||await realpath(file)!==file||before.dev!==after.dev||before.ino!==after.ino||before.size!==after.size||before.mtimeNs!==after.mtimeNs||before.ctimeNs!==after.ctimeNs||BigInt(bytes.length)!==after.size)throw Error("UI_NATIVE_LEAF_CHANGED_DURING_CAPTURE");
 return{binding:{path:relative(root,file).replaceAll("\\","/"),sha256:hash(bytes),bytes:bytes.length},bytes};
}
function sameBinding(a:UiEvidenceFile,b:UiEvidenceFile){return a.path===b.path&&a.sha256===b.sha256&&a.bytes===b.bytes;}
function caseDirectory(root:string,run:string,reportPath:string){
 if(!/^[a-f0-9]{12}$/.test(run))throw Error("UI_NATIVE_RUN_REQUIRED");const report=internal(root,reportPath),directory=dirname(report),parts=relative(join(root,".scratch/dev/test-suites/browser"),directory).split("/");
 if(parts.length!==2||parts.some(part=>!part||part==="."||part==="..")||basename(directory)!==run||basename(report)!=="result.json")throw Error("UI_NATIVE_CASE_DIRECTORY_REQUIRED");return directory;
}

/** Only explicit UI observation attachments opt in. Ordinary case receipts retain their old shape. */
export async function captureUiBrowserEvidence(input:{root:string;run:string;reportPath:string;report:unknown;runtimeFingerprint:string;verificationFingerprint:string;productHostLeasePath?:string}):Promise<UiBrowserEvidenceWitness|undefined>{
 const attached=attachments(input.report),markers=attached.filter(item=>item.name==="erpx-ui-observation");if(!markers.length)return undefined;
 const root=resolve(input.root);if(await realpath(root)!==root)throw Error("UI_NATIVE_ROOT_REQUIRED");const directory=caseDirectory(root,input.run,input.reportPath);if(await realpath(directory)!==directory)throw Error("UI_NATIVE_CASE_DIRECTORY_REQUIRED");
 const originalReport=await capture(root,input.reportPath,directory);if(JSON.stringify(JSON.parse(originalReport.bytes.toString()))!==JSON.stringify(input.report))throw Error("UI_ORIGINAL_REPORT_CHANGED");
 const cohort=join(root,".runtime/browser",input.run),liveState=await capture(root,join(cohort,"state.json"),cohort),state:unknown=JSON.parse(liveState.bytes.toString());
 if(!record(state)||state.run!==input.run||state.sourceFingerprint!==input.runtimeFingerprint||state.port!==3104||state.origin!=="http://127.0.0.1:3104"||state.ready!==true||!record(state.release))throw Error("UI_NATIVE_COMPILED_STATE_REQUIRED");
 let productHostLease:Capture|undefined,transport:Awaited<ReturnType<typeof import("./product-host-ui-evidence")["verifyProductHostUiLease"]>>|undefined;
 if(input.productHostLeasePath){
  productHostLease=await capture(root,input.productHostLeasePath,directory);
  const {readProductHostLease}=await import("./product-host-cohort");await readProductHostLease(root,input.productHostLeasePath);
  const {verifyProductHostUiLease}=await import("./product-host-ui-evidence"),{chromium}=await import("playwright");
  transport=await verifyProductHostUiLease({root,leaseBinding:productHostLease.binding,state:{...state,run:input.run,origin:String(state.origin),sourceFingerprint:String(state.sourceFingerprint),release:{id:String(state.release.id),fileManifestSha256:String(state.release.fileManifestSha256)}},expected:{run:input.run,sourceFingerprint:input.runtimeFingerprint,verificationFingerprint:input.verificationFingerprint,release:{id:String(state.release.id),fileManifestSha256:String(state.release.fileManifestSha256)},stateBinding:liveState.binding,chromiumPath:chromium.executablePath()}});
 }
 if(state.productHosts===true&&!transport)throw Error("UI_NATIVE_PRODUCT_HOST_LEASE_REQUIRED");
 const observed=new Map<string,UiEvidenceFile>(),screenshots=new Map<string,UiEvidenceFile>();
 for(const marker of markers){
  if(marker.contentType!=="application/json"||typeof marker.path!=="string")throw Error("UI_NATIVE_OBSERVATION_ATTACHMENT_REQUIRED");
  const leaf=await capture(root,marker.path,join(directory,"artifacts")),value:unknown=JSON.parse(leaf.bytes.toString());
  if(!record(value)||value.schema!=="erpx-ui-browser-observation/v1"||value.result!=="passed"||value.run!==input.run||value.sourceFingerprint!==input.runtimeFingerprint||value.verificationFingerprint!==input.verificationFingerprint||!record(value.screenshot)||typeof value.screenshot.path!=="string"||isAbsolute(value.screenshot.path))throw Error("UI_NATIVE_OBSERVATION_INVALID");
  let url:URL;try{url=new URL(String(value.url));}catch{throw Error("UI_NATIVE_OBSERVATION_ORIGIN_REQUIRED");}
  if(transport)transport.projectUrl(url.href);else if(url.origin!==state.origin||url.username||url.password)throw Error("UI_NATIVE_OBSERVATION_ORIGIN_REQUIRED");
  const image=await capture(root,value.screenshot.path,join(directory,"artifacts"));if(value.screenshot.sha256!==image.binding.sha256||value.screenshot.bytes!==image.binding.bytes||!attached.some(item=>item.contentType==="image/png"&&typeof item.path==="string"&&resolve(root,item.path)===resolve(root,image.binding.path)))throw Error("UI_NATIVE_SCREENSHOT_ATTACHMENT_REQUIRED");
  if(observed.has(leaf.binding.path))throw Error("UI_DUPLICATE_NATIVE_OBSERVATION");observed.set(leaf.binding.path,leaf.binding);screenshots.set(image.binding.path,image.binding);
 }
 // Capture the exact live state into the retained case before cleanup can destroy it.
 const retainedState=join(directory,"state.json");await copyFile(join(cohort,"state.json"),retainedState,constants.COPYFILE_EXCL);await chmod(retainedState,0o600);
 const copied=await capture(root,retainedState,directory);if(copied.binding.sha256!==liveState.binding.sha256||copied.binding.bytes!==liveState.binding.bytes)throw Error("UI_ORIGINAL_STATE_COPY_CHANGED");
 const witness:UiBrowserEvidenceWitness={schema:"erpx-ui-original-browser-leaves/v1",version:1,run:input.run,report:originalReport.binding,state:copied.binding,observations:[...observed.values()].sort((a,b)=>a.path.localeCompare(b.path)),screenshots:[...screenshots.values()].sort((a,b)=>a.path.localeCompare(b.path)),...(productHostLease?{productHostLease:productHostLease.binding}:{})};
 await verifyUiBrowserEvidence(root,witness,true);if(transport)await transport.recheck();return witness;
}

/** Recheck retained original bytes before and after normal cleanup; never rewrite the witnessed state. */
export async function verifyUiBrowserEvidence(rootInput:string,witness:UiBrowserEvidenceWitness,liveState=false):Promise<void>{
 const root=resolve(rootInput),directory=caseDirectory(root,witness.run,witness.report.path);
 if(witness.schema!=="erpx-ui-original-browser-leaves/v1"||witness.version!==1||witness.state.path!==relative(root,join(directory,"state.json")).replaceAll("\\","/"))throw Error("UI_ORIGINAL_WITNESS_INVALID");
 for(const binding of [witness.report,witness.state,...witness.observations,...witness.screenshots,...(witness.productHostLease?[witness.productHostLease]:[])]){
  const within=binding===witness.report||binding===witness.state||binding===witness.productHostLease?directory:join(directory,"artifacts"),actual=await capture(root,binding.path,within);if(!sameBinding(actual.binding,binding))throw Error("UI_ORIGINAL_WITNESSED_LEAF_CHANGED");
 }
 if(witness.productHostLease&&witness.productHostLease.path!==relative(root,join(directory,"product-host-lease.json")).replaceAll("\\","/"))throw Error("UI_ORIGINAL_PRODUCT_HOST_LEASE_REQUIRED");
 if(liveState){const cohort=join(root,".runtime/browser",witness.run),actual=await capture(root,join(cohort,"state.json"),cohort);if(actual.binding.sha256!==witness.state.sha256||actual.binding.bytes!==witness.state.bytes)throw Error("UI_ORIGINAL_STATE_COPY_CHANGED");}
}
