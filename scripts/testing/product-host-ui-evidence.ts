/** Offline verification of an ORIGINAL case's owned transport, not a browser
 * acceptance producer. Native route projection follows the current registry
 * and Proxy's existing root/Front/workspace rewrites. */
import {createHash,createPrivateKey,createPublicKey,X509Certificate} from "node:crypto";
import {createReadStream} from "node:fs";
import {lstat,readFile,realpath} from "node:fs/promises";
import {isAbsolute,join,resolve} from "node:path";
import {productIds,developmentProductOrigin} from "../../src/lib/commercial/product-registry";
import {sharedHostKeys} from "../../src/lib/commercial/product-registry";
import {productPageAllowed,productFrontBackofficeAllowed} from "../../src/lib/runtime/product-route-policy";
import {workspacePathParts} from "../../src/lib/work_projection/destination-registry";
import {productHostProducts,type ProductHostLease,type ProductHostPlan} from "./product-host-cohort";
type Binding={path:string;sha256:string;bytes?:number};
export type ProductHostUiState={run:string;prefix?:string;productHosts?:boolean;sourceFingerprint:string;softwareVersion?:string;release:{id:string;fileManifestSha256:string};ready?:boolean;diagnostic?:boolean;port?:number;origin:string;pid?:number;jobsPid?:number;jobsPaused?:boolean;webEntrypoint?:string;jobsEntrypoint?:string};
type Expected={root:string;run:string;sourceFingerprint:string;verificationFingerprint:string;release:{id:string;fileManifestSha256:string};stateBinding:Binding;chromiumPath:string};
const hash=(bytes:Uint8Array|string)=>createHash("sha256").update(bytes).digest("hex");
function assert(value:unknown,code:string):asserts value{if(!value)throw Error(code);}
const hosts=[...productIds,"account","apps","admin","legal-portal"] as const;
const origins=hosts.map(developmentProductOrigin);
const absolute=(root:string,path:string)=>isAbsolute(path)?path:resolve(root,path);
function boundShape(value:Binding){return value&&typeof value.path==="string"&&/^[a-f0-9]{64}$/.test(value.sha256)&&(value.bytes===undefined||Number.isSafeInteger(value.bytes)&&value.bytes>=0);}
function equalBinding(root:string,a:Binding,b:Binding){return boundShape(a)&&boundShape(b)&&absolute(root,a.path)===absolute(root,b.path)&&a.sha256===b.sha256;}
/** Metadata contracts are independently unit-tested; this function alone is
 * never a TLS/Files/browser attestation. verifyProductHostUiLease also checks
 * the real bound bytes, certificate chain/SPKI and exact executable files. */
export function assertProductHostUiMetadata(lease:ProductHostLease,plan:ProductHostPlan,state:ProductHostUiState,expected:Expected){
 const {root,run,release}=expected,ui=lease.ui;
 assert(JSON.stringify(productHostProducts)===JSON.stringify(productIds)&&sharedHostKeys.includes("legal-portal")&&JSON.stringify(lease.origins)===JSON.stringify(origins),"UI_PRODUCT_HOST_REGISTRY_REQUIRED");
 const businessWorkflow=lease.scopedBusinessWorkflow;
 assert(lease.schema==="erpx-owned-product-host-browser/v1"&&lease.run===run&&/^[a-f0-9]{12}$/.test(run)&&lease.sourceFingerprint===expected.sourceFingerprint&&lease.verificationFingerprint===expected.verificationFingerprint&&JSON.stringify(lease.release)===JSON.stringify(release)&&lease.port===3102&&lease.upstream==="http://127.0.0.1:3104"&&(businessWorkflow===null||businessWorkflow===plan.allowIsolatedBusinessWorkflow)&&(!businessWorkflow||lease.readOnlyWorkflow===null)&&(lease.readOnlyWorkflow===null||lease.readOnlyWorkflow==="admin.company-brand-organization-iam")&&lease.readOnlyBusiness===(businessWorkflow===null)&&lease.isolatedNormalCommercial===(lease.readOnlyWorkflow===null)&&lease.wholeProductAcceptance===false,"UI_PRODUCT_HOST_LEASE_SOURCE_REQUIRED");
 assert(state.run===run&&state.prefix===`erpx_browser_${run}`&&state.productHosts===true&&state.sourceFingerprint===expected.sourceFingerprint&&state.softwareVersion===`release-${expected.sourceFingerprint}`&&JSON.stringify(state.release)===JSON.stringify(release)&&state.ready===true&&!state.diagnostic&&state.port===3104&&state.origin==="http://127.0.0.1:3104"&&!state.jobsPaused,"UI_PRODUCT_HOST_BACKEND_REQUIRED");
 assert(ui&&ui.version===1&&ui.listener&&ui.backend&&ui.backend.state&&Number.isInteger(lease.pid)&&lease.pid>0&&ui.listener.address==="127.0.0.1"&&ui.listener.port===3102&&ui.listener.pid===lease.pid&&Number.isInteger(state.pid)&&Number(state.pid)>0&&Number.isInteger(state.jobsPid)&&Number(state.jobsPid)>0&&ui.backend.pid===state.pid&&ui.backend.jobsPid===state.jobsPid&&ui.backend.webEntrypoint===state.webEntrypoint&&ui.backend.jobsEntrypoint===state.jobsEntrypoint&&state.webEntrypoint===join(root,".runtime/browser",run,"release/server.js")&&state.jobsEntrypoint===join(root,".runtime/browser",run,"release/tools/jobs.mjs"),"UI_PRODUCT_HOST_OWNED_PROCESSES_REQUIRED");
 assert(ui.backend.state.path===join(root,".runtime/browser",run,"state.json")&&ui.backend.state.sha256===expected.stateBinding.sha256&&ui.backend.state.bytes===expected.stateBinding.bytes,"UI_PRODUCT_HOST_ORIGINAL_STATE_REQUIRED");
 const at=Date.parse(ui.observedAt);assert(Number.isFinite(at)&&at<=Date.now()&&plan&&plan.version===1&&plan.enabled===true&&Number.isSafeInteger(plan.expiresAt)&&plan.expiresAt>at&&plan.expiresAt-at<=12*3_600_000&&plan.runtimeFingerprint===expected.sourceFingerprint&&plan.verificationFingerprint===expected.verificationFingerprint&&plan.allowIsolatedNormalMfa===true&&plan.allowIsolatedNormalCommercial===true,"UI_PRODUCT_HOST_ORIGINAL_ARM_REQUIRED");
 assert(absolute(root,plan.release.path)===join(root,".runtime/releases",release.id,"release.json")&&absolute(root,plan.releaseFiles.path)===join(root,".runtime/releases",release.id,"release-files.json")&&plan.releaseFiles.sha256===release.fileManifestSha256,"UI_PRODUCT_HOST_COMPILED_ARTIFACT_REQUIRED");
 assert(plan.tls&&plan.tools&&boundShape(plan.tls.certificate)&&boundShape(plan.tls.key)&&boundShape(plan.tls.ca)&&/^[a-f0-9]{64}$/.test(plan.tls.spki)&&lease.certificateSha256===plan.tls.certificate.sha256&&lease.spki===plan.tls.spki&&equalBinding(root,lease.ca,plan.tls.ca)&&equalBinding(root,lease.chromium,plan.tools.chromium)&&absolute(root,lease.chromium.path)===expected.chromiumPath&&equalBinding(root,ui.node,plan.tools.node)&&absolute(root,ui.node.path)===join(root,".runtime/node/node-v26.11.1-darwin-arm64/bin/node")&&ui.source.path===join(root,"scripts/testing/product-host-cohort.ts")&&boundShape(ui.source),"UI_PRODUCT_HOST_TLS_AND_TOOLS_REQUIRED");
}
/** Returns the actual internal Next route pathname while preserving the exact
 * observed query/hash. It never substitutes another product or owner route. */
export function productHostUiProjection(value:string){
 const url=new URL(value),key=hosts.find(host=>developmentProductOrigin(host)===url.origin);assert(key&&!url.username&&!url.password&&url.protocol==="https:"&&productPageAllowed(key,url.pathname,"GET"),"UI_PRODUCT_HOST_ROUTE_REQUIRED");
 const path=url.pathname,front=key==="pos"||key==="pms";
 assert(!path.startsWith("/api/")&&!path.startsWith("/sso/")&&!(path==="/login"&&key!=="account"),"UI_PRODUCT_HOST_RENDERED_ROUTE_REQUIRED");
 if(path==="/"&&!front)url.pathname="/product-entry";
 else if(front&&!productFrontBackofficeAllowed(key,path,"GET")){
  if(["/launch","/choose-work-role"].includes(path))return url;
  if(["/","/forgot-password","/verify-email","/reset-password"].includes(path))url.pathname="/front-access"+(path==="/"?"":path);
  else {const parts=path.split("/").filter(Boolean);assert(parts.length>=2&&!['api','front','front-access','holdings','platform'].includes(parts[0]!)&&parts.every(part=>/^[A-Za-z0-9_-]+$/.test(part)),"UI_PRODUCT_HOST_FRONT_ROUTE_REQUIRED");url.pathname="/front/"+[parts[0],parts[1],key,...parts.slice(2)].join("/");}
 }else if(path.startsWith("/holdings/")){
  const scope=workspacePathParts(path);assert(scope,"UI_PRODUCT_HOST_WORKSPACE_ROUTE_REQUIRED");url.pathname="/holdings/"+scope.base.split("/")[2]+"/"+(scope.path||"home");
 }
 return url;
}
async function file(root:string,value:Binding,external?:string,privateFile=false){
 assert(boundShape(value),"UI_PRODUCT_HOST_BOUND_FILE_REQUIRED");const path=absolute(root,value.path);assert(resolve(path)===path&&!value.path.includes("\\")&&(path.startsWith(root+"/")||path===external)&&await realpath(path)===path,"UI_PRODUCT_HOST_BOUND_PATH_REQUIRED");
 const before=await lstat(path,{bigint:true});assert(before.isFile()&&!before.isSymbolicLink()&&(!privateFile||before.uid===BigInt(process.getuid?.()??-1)&&(before.mode&BigInt(0o077))===BigInt(0)),"UI_PRODUCT_HOST_PRIVATE_REGULAR_FILE_REQUIRED");
 const digest=createHash("sha256");for await(const chunk of createReadStream(path))digest.update(chunk);const after=await lstat(path,{bigint:true});assert(before.dev===after.dev&&before.ino===after.ino&&before.size===after.size&&before.mtimeNs===after.mtimeNs&&before.ctimeNs===after.ctimeNs&&digest.digest("hex")===value.sha256&&(value.bytes===undefined||BigInt(value.bytes)===after.size),"UI_PRODUCT_HOST_BOUND_BYTES_CHANGED");return path;
}
async function bytes(root:string,value:Binding,privateFile=false){const path=await file(root,value,undefined,privateFile);assert((await lstat(path)).size<=2*1024*1024,"UI_PRODUCT_HOST_BOUNDED_METADATA_REQUIRED");const raw=await readFile(path);assert(hash(raw)===value.sha256,"UI_PRODUCT_HOST_BOUND_BYTES_CHANGED");return raw;}
/** Historical verification: no PID probe, listener, URL request or native owner
 * is invoked after the original case has legitimately cleaned its cohort. */
export async function verifyProductHostUiLease(input:{root:string;leaseBinding:Binding;state:ProductHostUiState;expected:Omit<Expected,"root">}){
 const root=resolve(input.root),expected={root,...input.expected},lease=JSON.parse((await bytes(root,input.leaseBinding,true)).toString()) as ProductHostLease;
 assert(absolute(root,input.leaseBinding.path).endsWith('/'+expected.run+'/product-host-lease.json')&&absolute(root,input.leaseBinding.path).startsWith(join(root,".scratch/dev/test-suites/browser/")),"UI_PRODUCT_HOST_ORIGINAL_LEASE_PATH_REQUIRED");
 const plan=JSON.parse((await bytes(root,lease.plan,true)).toString()) as ProductHostPlan;assert(absolute(root,lease.plan.path).startsWith(join(root,".scratch/dev/")),"UI_PRODUCT_HOST_ORIGINAL_ARM_PATH_REQUIRED");
 assertProductHostUiMetadata(lease,plan,input.state,expected);
 const certBytes=await bytes(root,plan.tls.certificate),key=await bytes(root,plan.tls.key,true),caBytes=await bytes(root,plan.tls.ca),cert=new X509Certificate(certBytes),ca=new X509Certificate(caBytes),at=Date.parse(lease.ui!.observedAt),sans=origins.map(origin=>'DNS:'+new URL(origin).host).sort();
 assert(ca.ca&&cert.verify(ca.publicKey)&&Date.parse(ca.validFrom)<=at&&Date.parse(ca.validTo)>at&&Date.parse(cert.validFrom)<=at&&Date.parse(cert.validTo)>at&&JSON.stringify(cert.subjectAltName?.split(', ').sort())===JSON.stringify(sans)&&origins.every(origin=>cert.checkHost(new URL(origin).host)===new URL(origin).host)&&hash(cert.publicKey.export({format:'der',type:'spki'}))===lease.spki&&hash(createPublicKey(createPrivateKey(key)).export({format:'der',type:'spki'}))===lease.spki,"UI_PRODUCT_HOST_NATIVE_TLS_REQUIRED");
 const release=JSON.parse((await bytes(root,plan.release)).toString()) as {id:string;sourceFingerprint:string;node:string;fileManifestSha256:string};assert(release.id===expected.release.id&&release.sourceFingerprint===expected.sourceFingerprint&&release.node==='26.11.1'&&release.fileManifestSha256===expected.release.fileManifestSha256,"UI_PRODUCT_HOST_RELEASE_REQUIRED");await file(root,plan.releaseFiles);await file(root,plan.tools.node);await file(root,lease.ui!.source);await file(root,plan.tools.chromium,expected.chromiumPath);
 const retained=[input.leaseBinding,lease.plan,plan.tls.certificate,plan.tls.key,plan.tls.ca,plan.release,plan.releaseFiles,plan.tools.node,lease.ui!.source];
 return{lease,projectUrl:(value:string)=>{const url=productHostUiProjection(value);assert(lease.origins.includes(new URL(value).origin),"UI_PRODUCT_HOST_ORIGIN_NOT_LEASED");return url;},async recheck(){for(const binding of retained)await file(root,binding,undefined,binding===lease.plan||binding===plan.tls.key||binding===input.leaseBinding);await file(root,plan.tools.chromium,expected.chromiumPath);}};
}
