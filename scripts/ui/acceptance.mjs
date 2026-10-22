import {routeDependsOn,routeClosureIds} from "./inventory.mjs";
import {createHash} from 'node:crypto';
import {existsSync,lstatSync,readFileSync,realpathSync} from 'node:fs';
import {createReadStream} from 'node:fs';
import {resolve,dirname,isAbsolute,join,relative} from 'node:path';
import {spawnSync} from 'node:child_process';
import {inflateSync} from 'node:zlib';

const digest=bytes=>createHash('sha256').update(bytes).digest('hex');
const object=value=>value!==null&&typeof value==='object'&&!Array.isArray(value);
const text=value=>typeof value==='string'&&value.trim().length>0;
const sha=value=>typeof value==='string'&&/^[a-f0-9]{64}$/.test(value);
const same=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
const fail=code=>{throw Error(code);};
const widths=[320,390,768,1024,1440],themes=['light','dark','system'];
const formsHostOrigin='https://forms-dev-erpx.jesadakorn.com';
const formsHostWorkflow='forms.versioned-consent-submit-replay';
const formsHostSuite='marketing.forms-host.e2e';
const formsHostFile='tests/e2e/marketing/forms-host.spec.ts';
const formsHostTitle='isolated anonymous Forms host submits the published version and replays exact consent once';
const formsHostOwnerContext='Anonymous public visitor; no ERPX account or authenticated session';
const formsHostAuthorityBranch='published public marketing form';
const basicChecks=['long-thai','dense-data','currency','keyboard','focus','escape','dirty-reload','history','product-switch','loaded','loading','empty','error','validation','denied','stale','disabled/read-only','pagination/search/reset','portalled popup/draft focus','print if document'];
const sorted=values=>[...values].sort();
const unique=(values,code)=>{if(new Set(values).size!==values.length)fail(code);return values;};
const crcTable=Uint32Array.from({length:256},(_,value)=>{for(let bit=0;bit<8;bit++)value=value&1?0xedb88320^(value>>>1):value>>>1;return value>>>0;});
function crc32(bytes){let crc=0xffffffff;for(const value of bytes)crc=crcTable[(crc^value)&255]^(crc>>>8);return(crc^0xffffffff)>>>0;}
function pathName(value){if(!text(value)||isAbsolute(value)||value.includes('\\')||value.split('/').some(part=>!part||part==='.'||part==='..'))fail('UI_EVIDENCE_PATH_REQUIRED');return value;}
export function workspaceSourcePath(value){
 pathName(value);
 return /^(?:src\/|apps\/[a-z0-9_-]+\/src\/|packages\/[a-z0-9_-]+\/src\/)/.test(value);
}
function currentTarget(inventory,target){
 if(!object(target)||!['route','source','style'].includes(target.kind)||!text(target.source)||!sha(target.fingerprint))fail('UI_TARGET_BINDING_REQUIRED');
 const rows=target.kind==='route'?inventory.routes:target.kind==='style'?inventory.styleSources??[]:inventory.sources,found=rows.filter(row=>row.source===target.source);
 if(found.length!==1||found[0].fingerprint!==target.fingerprint)fail('UI_TARGET_SOURCE_CHANGED');return found[0];
}
/** Requirements come from the current inventory, never from a receipt's claimed omissions. */
export function uiTargetRequirements(inventory,target,protectedSources=[]){
 const row=currentTarget(inventory,target),sources=target.kind==='route'?inventory.sources.filter(source=>routeDependsOn(inventory,row,source.source)):target.kind==='style'?inventory.sources.filter(source=>(source.styles??[]).includes(target.source)):[row];
 const checks=new Set(basicChecks);
 for(const state of row.stateChecklist??[])checks.add(state);
 for(const query of [...(row.queryViews??[]),...sources.flatMap(source=>source.queryViews??[])])checks.add(`query:${query}`);
 for(const source of sources){
  for(const check of source.accessChecks??[]){const id=digest(check);checks.add(`authority:${id}:allowed`);checks.add(`authority:${id}:denied`);}
  for(const surface of source.surfaces??[])checks.add(`surface:${source.source}:${digest(JSON.stringify([surface.component,surface.line,surface.props,surface.conditions]))}`);
 }
 if(row.scope==='frozen-specialized'||row.scope==='deferred-pms-pos'||protectedSources.includes(row.source)||sources.some(source=>source.scope==='frozen-specialized'||protectedSources.includes(source.source)))checks.add('specialized-geometry-and-interaction');
 return sorted(checks);
}
function routeCallers(inventory,target){
 if(target.kind==='route')return[target.source];
 const callers=inventory.routes.filter(route=>route.source===target.source||routeDependsOn(inventory,route,target.source)||(target.kind==='style'&&(route.styles??[]).includes(target.source))).map(route=>route.source);
 if(!callers.length)fail('UI_SOURCE_HAS_NO_BROWSER_CALLER');return sorted(unique(callers,'UI_DUPLICATE_ROUTE_SOURCE'));
}
function routeMatches(route,url){
 const escape=value=>value.replace(/[.*+?^${}()|[\]\\]/g,'\\$&');
 const escaped=route.split('/').filter(Boolean).map(part=>/^\[\[\.\.\.[^\]]+\]\]$/.test(part)?'(?:/.*)?':/^\[\.\.\.[^\]]+\]$/.test(part)?'/.+':/^\[[^\]]+\]$/.test(part)?'/[^/]+':'/'+escape(part)).join('')||'/';
 return new RegExp(`^${escaped}/?$`).test(new URL(url).pathname);
}
function acceptedUrl(value,state,transport,formsLease){
 let url;try{url=new URL(value);}catch{fail('UI_OBSERVED_URL_REQUIRED');}
 if(url.username||url.password||state.origin!=='http://127.0.0.1:3104')fail('UI_OWNED_COMPILED_ORIGIN_REQUIRED');
 if(formsLease){
  const fixture=formsLease.fixture,expected=new URL(`/forms/${encodeURIComponent(fixture.slug)}/${encodeURIComponent(fixture.formId)}?utm_source=${encodeURIComponent(`Declared fictional Forms visitor ${formsLease.run}`)}`,formsHostOrigin);
  if(transport!==formsLease||state.formsHosts!==true||state.productHosts!==false||state.executiveWorkHosts!==false||url.href!==expected.href)fail('UI_FORMS_HOST_ROUTE_REQUIRED');
  return url;
 }
 if(transport)return transport.projectUrl(url.href);
 if(state.productHosts===true||url.origin!==state.origin)fail('UI_OWNED_COMPILED_ORIGIN_REQUIRED');return url;
}
function isFormsHostCase(nativeCase){return nativeCase.suite===formsHostSuite&&nativeCase.file===formsHostFile&&nativeCase.title===formsHostTitle;}
function validFormsHostLease(lease,nativeCase,state,observation,current){
 const evidence=lease?.requestEvidence,fixture=lease?.fixture,ui=lease?.ui,backend=ui?.backend,stateBinding=backend?.state;
 const statePath=typeof stateBinding?.path==='string'?stateBinding.path.replaceAll('\\','/') : '';
 return object(lease)&&lease.schema==='erpx-owned-forms-host-browser/v1'&&lease.run===nativeCase.run&&lease.workflow===formsHostWorkflow&&lease.origin===formsHostOrigin&&
  lease.formsOnly===true&&lease.productHosts===false&&lease.executiveWorkHosts===false&&lease.commercial===false&&lease.mfa===false&&lease.apply===false&&lease.packageEnforcement===false&&lease.wholeProductAcceptance===false&&
  lease.sourceFingerprint===current.runtimeFingerprint&&lease.verificationFingerprint===current.verificationFingerprint&&lease.release?.id===state.release?.id&&lease.release?.fileManifestSha256===state.release?.fileManifestSha256&&
  object(fixture)&&fixture.slug==='simulation'&&/^[A-Za-z0-9_-]{1,180}$/.test(fixture.formId??'')&&Number.isSafeInteger(fixture.formVersion)&&fixture.formVersion>0&&
  (fixture.mediaPublicationId===undefined||/^[A-Za-z0-9_-]{1,180}$/.test(fixture.mediaPublicationId))&&
  Number.isSafeInteger(lease.pid)&&lease.pid>0&&lease.port===3102&&lease.upstream==='http://127.0.0.1:3104'&&sha(lease.spki)&&sha(lease.certificateSha256)&&
  object(lease.plan)&&text(lease.plan.path)&&sha(lease.plan.sha256)&&object(lease.certificate)&&sha(lease.certificate.sha256)&&object(lease.key)&&sha(lease.key.sha256)&&object(lease.ca)&&sha(lease.ca.sha256)&&object(lease.chromium)&&sha(lease.chromium.sha256)&&
  object(lease.databases)&&text(lease.databases.path)&&sha(lease.databases.sha256)&&Number.isSafeInteger(lease.databases.bytes)&&lease.databases.bytes>0&&object(lease.fixtureGuide)&&text(lease.fixtureGuide.path)&&sha(lease.fixtureGuide.sha256)&&Number.isSafeInteger(lease.fixtureGuide.bytes)&&lease.fixtureGuide.bytes>0&&
  ui?.version===1&&ui.listener?.address==='127.0.0.1'&&ui.listener?.port===3102&&ui.listener?.pid===lease.pid&&
  Number.isSafeInteger(backend?.pid)&&backend.pid===state.pid&&Number.isSafeInteger(backend?.jobsPid)&&backend.jobsPid===state.jobsPid&&
  typeof backend?.webEntrypoint==='string'&&backend.webEntrypoint===state.webEntrypoint&&typeof backend?.jobsEntrypoint==='string'&&backend.jobsEntrypoint===state.jobsEntrypoint&&
  statePath.endsWith(`/.runtime/browser/${nativeCase.run}/state.json`)&&stateBinding.sha256===observation.state.sha256&&stateBinding.bytes===observation.state.bytes&&
  typeof ui.source?.path==='string'&&ui.source.path.endsWith('/scripts/testing/forms-host-cohort.ts')&&
  object(evidence)&&evidence.acceptedPosts===2&&evidence.replayCount===1&&evidence.refusalCount===0&&Array.isArray(evidence.refusalLog)&&evidence.refusalLog.length===0&&sha(evidence.bodySha256)&&sha(evidence.idempotencyKeySha256);
}
function originalCaseLeaves(nativeCase,observation){
 const witness=nativeCase.uiEvidence;
 if(!object(witness)||witness.schema!=='erpx-ui-original-browser-leaves/v1'||witness.version!==1||witness.run!==nativeCase.run||!Array.isArray(witness.observations)||!witness.observations.length||!Array.isArray(witness.screenshots)||!witness.screenshots.length)fail('UI_ORIGINAL_CASE_LEAF_WITNESS_REQUIRED');
 const sameLeaf=(claimed,original)=>object(claimed)&&object(original)&&text(original.path)&&sha(original.sha256)&&Number.isSafeInteger(original.bytes)&&original.bytes>0&&claimed.path===original.path&&claimed.sha256===original.sha256&&claimed.bytes===original.bytes;
 const observed=witness.observations.filter(binding=>binding.path===observation.observation?.path),images=witness.screenshots.filter(binding=>binding.path===observation.screenshot?.path);
 if(!sameLeaf(observation.report,witness.report)||!sameLeaf(observation.state,witness.state)||observed.length!==1||images.length!==1||!sameLeaf(observation.observation,observed[0])||!sameLeaf(observation.screenshot,images[0]))fail('UI_ORIGINAL_CASE_LEAF_CHANGED');
 if(witness.productHostLease!==undefined&&!sameLeaf(nativeCase.productHostLease,witness.productHostLease))fail('UI_ORIGINAL_PRODUCT_HOST_LEASE_CHANGED');
 if(witness.formsHostLease!==undefined&&!sameLeaf(nativeCase.formsHostLease,witness.formsHostLease))fail('UI_ORIGINAL_FORMS_HOST_LEASE_CHANGED');
 if(nativeCase.formsHostLease!==undefined&&witness.formsHostLease===undefined)fail('UI_ORIGINAL_FORMS_HOST_LEASE_REQUIRED');
 for(const binding of [witness.report,witness.state,...witness.observations,...witness.screenshots,...(witness.productHostLease?[witness.productHostLease]:[]),...(witness.formsHostLease?[witness.formsHostLease]:[])])pathName(binding.path);
}
function pngSize(bytes){
 if(bytes.length<45||!bytes.subarray(0,8).equals(Buffer.from([137,80,78,71,13,10,26,10]))||bytes.readUInt32BE(8)!==13||bytes.subarray(12,16).toString()!=='IHDR')fail('UI_NATIVE_PNG_REQUIRED');
 const width=bytes.readUInt32BE(16),height=bytes.readUInt32BE(20),channels=bytes[25]===2?3:bytes[25]===6?4:0;
 if(!widths.includes(width)||!Number.isSafeInteger(height)||height<320||height>4096||bytes[24]!==8||!channels||bytes[26]!==0||bytes[27]!==0||bytes[28]!==0)fail('UI_NATIVE_PNG_FORMAT_REQUIRED');
 const data=[];let offset=8,ended=false;
 while(offset+12<=bytes.length){const length=bytes.readUInt32BE(offset),kind=bytes.subarray(offset+4,offset+8).toString(),end=offset+12+length;if(end>bytes.length)fail('UI_NATIVE_PNG_TRUNCATED');if(crc32(bytes.subarray(offset+4,end-4))!==bytes.readUInt32BE(end-4))fail('UI_NATIVE_PNG_CRC_INVALID');if(kind==='IDAT')data.push(bytes.subarray(offset+8,end-4));if(kind==='IEND'){if(length!==0||end!==bytes.length)fail('UI_NATIVE_PNG_TRUNCATED');ended=true;break;}offset=end;}
 if(!ended||!data.length)fail('UI_NATIVE_PNG_TRUNCATED');const row=width*channels+1,decoded=inflateSync(Buffer.concat(data),{maxOutputLength:row*height});if(decoded.length!==row*height)fail('UI_NATIVE_PNG_TRUNCATED');for(let y=0;y<height;y++)if(decoded[y*row]>4)fail('UI_NATIVE_PNG_FILTER_INVALID');return{width,height};
}

/** Pure evidence-contract evaluation. Production ports below always read and verify the real bound artifacts.
 * Injected ports in units are metadata contract fixtures, never a native/browser acceptance producer. */
export async function validateUiAcceptance({index,inventory,frozen,retirements,current},ports){
 if(!object(index)||index.version!==1||!Array.isArray(index.records)||Object.keys(index).some(key=>!['version','records'].includes(key)))fail('UI_ACCEPTANCE_INDEX_INVALID');
 const sources=new Map(),routes=new Map(),transitions=new Map(),pendingTransitions=[],ids=new Set();
 const closurePaths=inventory.closurePaths,closurePathsValid=Array.isArray(closurePaths)&&closurePaths.every(text)&&same(closurePaths,sorted(new Set(closurePaths)));
 for(const entry of index.records){
  if(!object(entry)||!text(entry.id)||ids.has(entry.id)||!entry.path?.startsWith('docs/testing/')||!sha(entry.sha256))fail('UI_ACCEPTANCE_INDEX_RECORD_INVALID');ids.add(entry.id);
  const receipt=ports.json(entry);
  if(!object(receipt)||receipt.schema!=='erpx-ui-acceptance-receipt/v1'||receipt.id!==entry.id||receipt.result!=='passed'||receipt.sourceFingerprint!==current.runtimeFingerprint||receipt.verificationFingerprint!==current.verificationFingerprint||receipt.candidate!==current.candidate)fail('UI_CURRENT_CANDIDATE_RECEIPT_REQUIRED');
  if(!object(receipt.regression)||!Array.isArray(receipt.regression.browsers)||!receipt.regression.browsers.length||[receipt.regression.native,receipt.regression.aggregate,receipt.regression.release,receipt.regression.releaseFiles,...receipt.regression.browsers].some(binding=>!object(binding)||!text(binding.path)||!sha(binding.sha256)))fail('UI_NATIVE_AND_BROWSER_REGRESSION_BINDINGS_REQUIRED');
  const candidate=await ports.candidate(receipt);
  if(!candidate||candidate.passed!==true||candidate.runtimeFingerprint!==current.runtimeFingerprint||candidate.verificationFingerprint!==current.verificationFingerprint||candidate.candidate!==current.candidate||!text(candidate.release?.id)||!sha(candidate.release.fileManifestSha256))fail('UI_FULL_SAME_CANDIDATE_REGRESSION_REQUIRED');
  const visual=ports.json(receipt.visualReview),images=new Set();if(!object(visual)||visual.schema!=='erpx-ui-visual-review/v1'||visual.result!=='passed'||visual.sourceFingerprint!==current.runtimeFingerprint||visual.verificationFingerprint!==current.verificationFingerprint||visual.candidate!==current.candidate||visual.releaseId!==candidate.release.id||!text(visual.reviewer)||!Array.isArray(visual.reviewedImages)||!visual.reviewedImages.length||visual.reviewedImages.some(value=>!sha(value))||!Array.isArray(visual.findings)||visual.findings.some(finding=>!object(finding)||!['fixed','observation'].includes(finding.severity)||!text(finding.detail)))fail('UI_CURRENT_VISUAL_REVIEW_REQUIRED');unique(visual.reviewedImages,'UI_DUPLICATE_VISUAL_REVIEW_IMAGE');
  if(!Array.isArray(receipt.targets)||!receipt.targets.length||!Array.isArray(receipt.transitions))fail('UI_TARGET_MATRIX_REQUIRED');
  for(const target of receipt.targets){
   const row=currentTarget(inventory,target),claims=target.kind==='route'?routes:sources;
   if(claims.has(target.source)||!Array.isArray(target.observations)||!target.observations.length)fail('UI_DUPLICATE_OR_EMPTY_TARGET');
   if(target.kind==='route'){
    const closureIds=routeClosureIds(inventory,row),closure=closurePathsValid&&Array.isArray(closureIds)&&closureIds.every(id=>Number.isSafeInteger(id)&&id>=0&&id<closurePaths.length)&&same(closureIds,[...new Set(closureIds)].sort((a,b)=>a-b))?closureIds.map(id=>closurePaths[id]):null;
    if(!Array.isArray(closure)||!closure.includes(row.source)||!same(closure,sorted(new Set(closure))))fail('UI_ROUTE_CLOSURE_REQUIRED');
    if(!same(row.styles??[],closure.filter(path=>path.endsWith('.css'))))fail('UI_ROUTE_STYLE_CLOSURE_REQUIRED');
    // Preserve the existing inventory fingerprint algorithm, including its style entries.
    const actual=digest([...closure,...(row.styles??[])].map(path=>`${path}:${digest(ports.source(path))}`).sort().join('\n'));if(actual!==row.fingerprint)fail('UI_ROUTE_CLOSURE_CHANGED');
   }else if(digest(ports.source(row.source))!==row.fingerprint)fail('UI_TARGET_SOURCE_CHANGED');
   const callers=routeCallers(inventory,target),requirements=uiTargetRequirements(inventory,target,frozen.map(file=>file.source)),covered=new Map(callers.map(caller=>[caller,{cells:new Map(),system:new Set()}]));
   for(const observation of target.observations){
    if(!object(observation)||!object(observation.case)||!text(observation.case.file)||!text(observation.case.title)||!['desktop','mobile'].includes(observation.case.project))fail('UI_EXACT_BROWSER_CASE_REQUIRED');
    const matches=candidate.browserCases?.filter(test=>test.file===observation.case.file&&test.title===observation.case.title&&test.project===observation.case.project&&test.passed===true)??[];
    if(matches.length!==1)fail('UI_CASE_NOT_IN_FULL_REGRESSION');const nativeCase=matches[0];
    originalCaseLeaves(nativeCase,observation);
    const report=ports.json(observation.report),actual=ports.json(observation.observation);
    const state=ports.json(observation.state);ports.caseWitness(nativeCase,observation);
    if(!/^[a-f0-9]{12}$/.test(nativeCase.run??'')||actual.run!==nativeCase.run||nativeCase.cleanup?.completed!==true||nativeCase.cleanup.ownedDirectoryAbsent!==true||state.run!==nativeCase.run||state.sourceFingerprint!==current.runtimeFingerprint||state.softwareVersion!==`release-${current.runtimeFingerprint}`||state.release?.id!==candidate.release.id||state.release.fileManifestSha256!==candidate.release.fileManifestSha256||state.ready!==true||state.diagnostic||state.port!==3104||state.origin!=='http://127.0.0.1:3104'||!Number.isInteger(state.pid)||state.pid<1||!Number.isInteger(state.jobsPid)||state.jobsPid<1||state.jobsPaused===true||!state.webEntrypoint?.endsWith(`/.runtime/browser/${nativeCase.run}/release/server.js`)||!state.jobsEntrypoint?.endsWith(`/.runtime/browser/${nativeCase.run}/release/tools/jobs.mjs`))fail('UI_NATIVE_COMPILED_COHORT_REQUIRED');
    ports.browserCase(report,observation.case,observation.observation,observation.screenshot);
    if(!object(actual)||actual.schema!=='erpx-ui-browser-observation/v1'||actual.result!=='passed'||actual.sourceFingerprint!==current.runtimeFingerprint||actual.verificationFingerprint!==current.verificationFingerprint||actual.candidate!==current.candidate||actual.releaseId!==candidate.release.id||actual.releaseManifestSha256!==candidate.release.fileManifestSha256||actual.targetSource!==target.source||actual.targetKind!==target.kind||actual.targetFingerprint!==target.fingerprint||!covered.has(actual.routeSource))fail('UI_BROWSER_OBSERVATION_SOURCE_MISMATCH');
    const formsClaim=state.formsHosts===true||nativeCase.formsHostLease!==undefined||nativeCase.uiEvidence.formsHostLease!==undefined||actual.authentication==='anonymous-public-visitor'||typeof actual.url==='string'&&actual.url.startsWith(formsHostOrigin),formsCase=isFormsHostCase(nativeCase);
    let transport,formsLease;
    if(formsClaim||formsCase){
     if(!formsCase||state.formsHosts!==true||state.profile!=='engineering'||state.productHosts!==false||state.executiveWorkHosts!==false||nativeCase.productHostLease!==undefined||nativeCase.executiveWorkHostLease!==undefined||nativeCase.uiEvidence.productHostLease!==undefined||nativeCase.uiEvidence.executiveWorkHostLease!==undefined)fail('UI_FORMS_HOST_EXACT_CASE_REQUIRED');
     if(!nativeCase.formsHostLease||!nativeCase.uiEvidence.formsHostLease||typeof ports.formsHost!=='function')fail('UI_ORIGINAL_FORMS_HOST_LEASE_REQUIRED');
     formsLease=await ports.formsHost(nativeCase,state,observation);if(!validFormsHostLease(formsLease,nativeCase,state,observation,current))fail('UI_FORMS_HOST_LEASE_SOURCE_REQUIRED');transport=formsLease;
    }else if(state.productHosts===true||nativeCase.productHostLease!==undefined||nativeCase.uiEvidence.productHostLease!==undefined){if(typeof ports.productHost!=='function'||!nativeCase.uiEvidence.productHostLease)fail('UI_ORIGINAL_PRODUCT_HOST_LEASE_REQUIRED');transport=await ports.productHost(nativeCase,state,observation);}
    const url=acceptedUrl(actual.url,state,transport,formsLease),caller=inventory.routes.find(route=>route.source===actual.routeSource);
    const ownerEntry=!!caller&&routeMatches(caller.route,url.href)&&text(actual.ownerContext)&&text(actual.authorityBranch)&&
     (formsLease?actual.authentication==='anonymous-public-visitor'&&actual.ownerContext===formsHostOwnerContext&&actual.authorityBranch===formsHostAuthorityBranch:actual.authentication==='normal-native-owner');
    if(!ownerEntry)fail(formsLease?'UI_FORMS_HOST_PUBLIC_VISITOR_REQUIRED':'UI_BROWSER_OWNER_ENTRY_REQUIRED');
    if(!object(actual.viewport)||!widths.includes(actual.viewport.width)||!Number.isSafeInteger(actual.viewport.height)||actual.viewport.height<1||actual.deviceScaleFactor!==1||!themes.includes(actual.theme)||!['light','dark'].includes(actual.resolvedTheme)||actual.theme!=='system'&&actual.resolvedTheme!==actual.theme)fail('UI_NATIVE_VIEWPORT_THEME_REQUIRED');
    if(!Array.isArray(actual.checks)||!actual.checks.length||actual.checks.some(check=>!requirements.includes(check))||new Set(actual.checks).size!==actual.checks.length)fail('UI_OBSERVED_REQUIREMENT_KEYS_INVALID');
    if(!same(actual.screenshot,observation.screenshot)||!visual.reviewedImages.includes(observation.screenshot?.sha256))fail('UI_SCREENSHOT_BINDING_MISMATCH');images.add(observation.screenshot.sha256);const size=pngSize(ports.bytes(observation.screenshot));if(size.width!==actual.viewport.width||size.height!==actual.viewport.height)fail('UI_NATIVE_VIEWPORT_IMAGE_MISMATCH');
    const coverage=covered.get(actual.routeSource),cell=`${actual.viewport.width}:${actual.theme}`,checks=coverage.cells.get(cell)??new Set();for(const check of actual.checks)checks.add(check);coverage.cells.set(cell,checks);if(actual.theme==='system')coverage.system.add(actual.resolvedTheme);
   }
   for(const coverage of covered.values())if(widths.some(width=>themes.some(theme=>requirements.some(check=>!coverage.cells.get(`${width}:${theme}`)?.has(check))))||!coverage.system.has('light')||!coverage.system.has('dark'))fail('UI_REQUIRED_BROWSER_MATRIX_INCOMPLETE');
   claims.set(target.source,{fingerprint:row.fingerprint,receipt:entry,callers});
  }
  if(!same(sorted(images),sorted(visual.reviewedImages)))fail('UI_VISUAL_REVIEW_IMAGE_SET_MISMATCH');
  for(const transition of receipt.transitions)pendingTransitions.push({transition,entry});
 }
 for(const {transition,entry} of pendingTransitions){
  if(!object(transition)||transition.purpose!=='authorized-product-migration'||transitions.has(transition.source)||!sha(transition.originalFingerprint)||!/^[a-f0-9]{40}$/.test(transition.originalCommit??''))fail('UI_MIGRATION_TRANSITION_INVALID');
  const original=frozen.find(file=>file.source===transition.source),retired=retirements.find(file=>file.source===transition.source);
  if(!original||transition.originalFingerprint!==original.fingerprint||digest(ports.git(transition.originalCommit,transition.source))!==original.fingerprint)fail('UI_ORIGINAL_FROZEN_EVIDENCE_REQUIRED');
  if(!same(transition.originalSuccessors,retired?.successors??[])||retired&&transition.originalCommit!==retired.originalCommit)fail('UI_RETIRED_ORIGINAL_SUCCESSORS_CHANGED');
  if(!object(transition.current)||!['present','absent'].includes(transition.current.state)||transition.current.state==='absent'&&!retired||!Array.isArray(transition.successors)||!transition.successors.length)fail('UI_TRANSITION_SUCCESSOR_CLOSURE_REQUIRED');
  if(transition.current.state==='present'){
   if(transition.current.binding?.path!==transition.source||!sources.has(transition.source)||sources.get(transition.source).fingerprint!==transition.current.binding.sha256)fail('UI_TRANSITION_CURRENT_SOURCE_UNACCEPTED');ports.bytes(transition.current.binding);
  }else if(ports.exists(transition.source))fail('UI_RETIRED_ENTRYPOINT_REAPPEARED');
  const successorHashes=new Map();for(const successor of transition.successors){
   if(!workspaceSourcePath(successor.path)||successorHashes.has(successor.path)||!sources.has(successor.path)||sources.get(successor.path).fingerprint!==successor.sha256)fail('UI_TRANSITION_SUCCESSOR_UNACCEPTED');ports.bytes(successor);successorHashes.set(successor.path,successor.sha256);
  }
  if(retired&&retired.successors.some(successor=>!successorHashes.has(successor.source)))fail('UI_TRANSITION_RETIRED_SUCCESSOR_OMITTED');
  const callers=sorted(new Set([...successorHashes.keys(),...(transition.current.state==='present'?[transition.source]:[])].flatMap(source=>inventory.routes.filter(route=>route.source===source||routeDependsOn(inventory,route,source)||(route.styles??[]).includes(source)).map(route=>route.source))));
  if(!callers.length||!same(transition.routeSources,callers)||callers.some(source=>!routes.has(source)))fail('UI_TRANSITION_CALLER_CLOSURE_UNACCEPTED');
  transitions.set(transition.source,{receipt:entry,current:transition.current,successors:successorHashes});
 }
 return{sources,routes,transitions};
}

function localFile(root,path){const name=pathName(path),file=resolve(root,name);if(realpathSync(file)!==file||!lstatSync(file).isFile())fail('UI_REGULAR_INTERNAL_EVIDENCE_REQUIRED');return file;}
function readBinding(root,binding){if(!object(binding)||!sha(binding.sha256))fail('UI_FILE_BINDING_REQUIRED');const bytes=readFileSync(localFile(root,binding.path));if(digest(bytes)!==binding.sha256||binding.bytes!==undefined&&binding.bytes!==bytes.length)fail('UI_BOUND_EVIDENCE_CHANGED');return bytes;}
async function fileDigest(file){const hash=createHash('sha256');for await(const chunk of createReadStream(file))hash.update(chunk);return hash.digest('hex');}
function reportAttachments(report){const rows=[];function walk(suite){for(const spec of suite.specs??[])for(const test of spec.tests??[])for(const result of test.results??[])rows.push(...(result.attachments??[]));for(const child of suite.suites??[])walk(child);}for(const suite of report.suites??[])walk(suite);return rows;}

/** No receipt => no import/fingerprint/build scan, preserving legacy isolated guard fixtures. */
export async function loadUiAcceptance(root,inventory){
 const register=resolve(root,'docs/ux-ui/acceptance-receipts.json'),empty={sources:new Map(),routes:new Map(),transitions:new Map()};if(!existsSync(register))return empty;
 const indexBytes=readFileSync(localFile(root,'docs/ux-ui/acceptance-receipts.json')),index=JSON.parse(indexBytes);
 if(!object(index)||index.version!==1||!Array.isArray(index.records)||Object.keys(index).some(key=>!['version','records'].includes(key)))fail('UI_ACCEPTANCE_INDEX_INVALID');if(!index.records.length)return empty;
 const fingerprints=await import('../local/source-fingerprint.ts'),runtimeFingerprint=await fingerprints.runtimeSourceFingerprint(root),verificationFingerprint=await fingerprints.verificationSourceFingerprint(root,runtimeFingerprint),git=spawnSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'});if(git.status!==0||!/^[a-f0-9]{40}$/.test(git.stdout.trim()))fail('UI_CURRENT_GIT_CANDIDATE_REQUIRED');
 const current={runtimeFingerprint,verificationFingerprint,candidate:git.stdout.trim()},frozen=JSON.parse(readFileSync(localFile(root,'scripts/ui/frozen-boundaries.json'))),retirements=existsSync(resolve(root,'scripts/ui/frozen-retirements.json'))?JSON.parse(readFileSync(localFile(root,'scripts/ui/frozen-retirements.json'))):[];
 const [{aggregateCiRegression},{verifyBrowserEvidenceArtifacts},{internalRequiredCases,loadRequiredCases},{assertBrowserCasePassed}]=await Promise.all([import('../testing/browser-sharding.ts'),import('../testing/browser-evidence-artifacts.ts'),import('../testing/required-case-inventory.ts'),import('../testing/browser-results.ts')]);
 const catalogue=JSON.parse(readFileSync(localFile(root,'tests/catalogue/suite-manifest.json'))),required=internalRequiredCases(loadRequiredCases(root),catalogue.suites),used=new Map(),artifacts=new Map(),productTransports=new Map(),formsTransports=new Map(),bytes=binding=>{const raw=readBinding(root,binding);used.set(binding.path,{...binding,sha256:digest(raw)});return raw;},json=binding=>JSON.parse(bytes(binding));
 const accepted=await validateUiAcceptance({index,inventory,frozen,retirements,current},{bytes,json,source:path=>readFileSync(localFile(root,path)),exists:path=>existsSync(resolve(root,path)),git(commit,path){if(!workspaceSourcePath(path))fail('UI_ORIGINAL_SOURCE_PATH_REQUIRED');pathName(path);const result=spawnSync('git',['show',`${commit}:${path}`],{cwd:root});if(result.status!==0)fail('UI_ORIGINAL_GIT_SOURCE_UNAVAILABLE');return result.stdout;},
  caseWitness(nativeCase,observation){if(!text(nativeCase.report)||resolve(root,nativeCase.report)!==resolve(root,observation.report.path)||resolve(root,observation.state.path)!==resolve(dirname(nativeCase.report),'state.json'))fail('UI_EXACT_CASE_RETAINED_STATE_REQUIRED');},
  async productHost(nativeCase,state,observation){
   const lease=nativeCase.uiEvidence.productHostLease;if(resolve(root,lease.path)!==resolve(dirname(nativeCase.report),'product-host-lease.json'))fail('UI_EXACT_CASE_PRODUCT_HOST_LEASE_REQUIRED');bytes(lease);
   const key=JSON.stringify([lease,observation.state]),cached=productTransports.get(key);if(cached)return cached;
   const [{verifyProductHostUiLease},{chromium}]=await Promise.all([import('../testing/product-host-ui-evidence.ts'),import('playwright')]);
   const verified=await verifyProductHostUiLease({root,leaseBinding:lease,state,expected:{run:nativeCase.run,sourceFingerprint:current.runtimeFingerprint,verificationFingerprint:current.verificationFingerprint,release:state.release,stateBinding:observation.state,chromiumPath:chromium.executablePath()}});productTransports.set(key,verified);return verified;
  },
  async formsHost(nativeCase,state,observation){
   const lease=nativeCase.uiEvidence.formsHostLease;if(resolve(root,lease.path)!==resolve(dirname(nativeCase.report),'forms-host-lease.json'))fail('UI_EXACT_CASE_FORMS_HOST_LEASE_REQUIRED');bytes(lease);
   const key=JSON.stringify([lease,observation.state]),cached=formsTransports.get(key);if(cached)return cached;
   const [{readFormsHostLease},{chromium}]=await Promise.all([import('../testing/forms-host-cohort.ts'),import('playwright')]);
   const verified=await readFormsHostLease(root,resolve(root,lease.path));
   const stateBytes=bytes(observation.state),retainedState=JSON.parse(stateBytes.toString('utf8')),statePath=resolve(root,'.runtime/browser',nativeCase.run,'state.json');
   if(!same(retainedState,state))fail('UI_FORMS_HOST_RETAINED_STATE_CHANGED');
   if(verified.run!==nativeCase.run||verified.sourceFingerprint!==current.runtimeFingerprint||verified.verificationFingerprint!==current.verificationFingerprint||verified.release.id!==state.release?.id||verified.release.fileManifestSha256!==state.release?.fileManifestSha256||
      verified.ui.backend.state.path!==statePath||verified.ui.backend.state.sha256!==digest(stateBytes)||verified.ui.backend.state.bytes!==stateBytes.length||
      verified.ui.backend.pid!==state.pid||verified.ui.backend.jobsPid!==state.jobsPid||verified.ui.backend.webEntrypoint!==state.webEntrypoint||verified.ui.backend.jobsEntrypoint!==state.jobsEntrypoint||
      verified.ui.source.path!==join(root,'scripts/testing/forms-host-cohort.ts')||verified.chromium.path!==chromium.executablePath())fail('UI_FORMS_HOST_ORIGINAL_COHORT_REQUIRED');
   for(const binding of [verified.plan,verified.certificate,verified.key,verified.ca,verified.chromium,verified.databases,verified.fixtureGuide,verified.ui.node,verified.ui.source])
    bytes({path:relative(root,binding.path),sha256:binding.sha256,...('bytes'in binding?{bytes:binding.bytes}:{})});
   formsTransports.set(key,verified);return verified;
  },
  browserCase(report,wanted,observation,screenshot){assertBrowserCasePassed(report,wanted);const attachments=reportAttachments(report);for(const binding of [observation,screenshot])if(!attachments.some(item=>text(item.path)&&resolve(root,item.path)===resolve(root,binding.path)))fail('UI_BROWSER_REPORT_ATTACHMENT_REQUIRED');},
  async candidate(receipt){
   const proof=receipt.regression;if(!object(proof)||!Array.isArray(proof.browsers)||!proof.browsers.length)fail('UI_NATIVE_AND_BROWSER_REGRESSION_BINDINGS_REQUIRED');
   const native=json(proof.native),browsers=proof.browsers.map(json),submitted=json(proof.aggregate);for(const browser of browsers)await verifyBrowserEvidenceArtifacts(browser,resolve(root,'.scratch/dev/test-suites'));const aggregate=aggregateCiRegression({native,browsers,candidate:current.candidate,total:browsers[0].shard.total,requiredSuiteIds:catalogue.suites.filter(suite=>['e2e','journeys','uat'].includes(suite.lane)).map(suite=>suite.id),requiredCases:required});
   if(!same(submitted,aggregate)||aggregate.runtimeFingerprint!==runtimeFingerprint||aggregate.verificationFingerprint!==verificationFingerprint)fail('UI_FULL_CURRENT_REGRESSION_REQUIRED');
   for(const browser of browsers)for(const test of browser.receipts)if(test.uiEvidence!==undefined){const originals=browser.isolation.forward.receipts.filter(original=>original.suite===test.suite&&original.file===test.file&&original.title===test.title&&original.project===test.project&&original.run===test.run);if(originals.length!==1||!same(originals[0].uiEvidence,test.uiEvidence)||test.uiEvidence.productHostLease!==undefined&&!same(originals[0].productHostLease,test.productHostLease)||test.uiEvidence.formsHostLease!==undefined&&!same(originals[0].formsHostLease,test.formsHostLease))fail('UI_NATIVE_ORIGINAL_WITNESS_MISMATCH');}
   for(const lane of ['unit','integration'])for(const suite of catalogue.suites.filter(suite=>suite.lane===lane))for(const file of suite.files)if(!native.cases.some(test=>test.file===file&&test.status==='passed'))fail('UI_FULL_NATIVE_FILE_COVERAGE_REQUIRED');
   const release=json(proof.release),filesBytes=bytes(proof.releaseFiles),files=JSON.parse(filesBytes);if(release.sourceFingerprint!==runtimeFingerprint||release.node!=='26.11.1'||release.postgres!=='18.6'||release.id!==aggregate.release.id||release.fileManifestSha256!==aggregate.release.fileManifestSha256||digest(filesBytes)!==release.fileManifestSha256||!object(files)||!Object.keys(files).length)fail('UI_SAME_COMPILED_RELEASE_REQUIRED');
   const directory=dirname(resolve(root,proof.release.path));if(!directory.startsWith(resolve(root,'.runtime')+'/')||proof.release.path.split('/').at(-1)!=='release.json'||proof.releaseFiles.path!==proof.release.path.replace(/release\.json$/,'release-files.json'))fail('UI_EXACT_RELEASE_METADATA_REQUIRED');
   for(const [name,expected] of Object.entries(files)){pathName(name);const file=resolve(directory,name),actual=realpathSync(file);if(!actual.startsWith(directory+'/')||!lstatSync(actual).isFile()||!sha(expected)||await fileDigest(actual)!==expected)fail('UI_COMPILED_RELEASE_FILE_CHANGED');artifacts.set(file,{actual,sha256:expected});}
   return{...aggregate,browserCases:browsers.flatMap(browser=>browser.receipts)};
  }
 });
 for(const transport of productTransports.values())await transport.recheck();
 const finalGit=spawnSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'});if(!readFileSync(register).equals(indexBytes)||finalGit.status!==0||finalGit.stdout.trim()!==current.candidate||await fingerprints.runtimeSourceFingerprint(root)!==runtimeFingerprint||await fingerprints.verificationSourceFingerprint(root,runtimeFingerprint)!==verificationFingerprint)fail('UI_ACCEPTANCE_SOURCE_CHANGED_DURING_READ');for(const binding of used.values())readBinding(root,binding);for(const [path,binding]of artifacts)if(realpathSync(path)!==binding.actual||await fileDigest(binding.actual)!==binding.sha256)fail('UI_COMPILED_RELEASE_CHANGED_DURING_READ');return accepted;
}

/** Regeneration revalidates receipts; it never preserves a manually edited accepted string. */
export function applyUiAcceptance(inventory,accepted){
 for(const source of inventory.sources){source.verification=source.scope?.startsWith('deferred')||source.scope==='frozen-specialized'?'excluded':'pending';if(accepted.sources.get(source.source)?.fingerprint===source.fingerprint){source.verification='accepted';source.acceptanceEvidence=accepted.sources.get(source.source).receipt;}else delete source.acceptanceEvidence;}
 for(const route of inventory.routes){route.verification=route.scope?.startsWith('deferred')||route.scope==='frozen-specialized'?'excluded':'pending';route.evidence=[];if(accepted.routes.get(route.source)?.fingerprint===route.fingerprint){route.verification='accepted';route.evidence=[accepted.routes.get(route.source).receipt];}}
 return inventory;
}
