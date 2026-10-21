import type {Page,TestInfo} from '@playwright/test';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {existsSync,lstatSync,readFileSync,realpathSync,writeFileSync} from 'node:fs';
import {dirname,relative,resolve,sep} from 'node:path';
import {runtimeSourceFingerprint,verificationSourceFingerprint} from '../../../scripts/local/source-fingerprint';
import type {ExecutiveWorkHostLease} from '../../../scripts/testing/executive-work-host-cohort';
import {formsHostOrigin,readFormsHostLease} from '../../../scripts/testing/forms-host-cohort';

const compiledOrigin='http://127.0.0.1:3104';
const executiveWorkOrigin='https://work-dev-erpx.jesadakorn.com';
const browserEvidenceRoot='.scratch/dev/test-suites/browser';
const coveragePath='docs/ux-ui/standard-coverage.json';
const hash=(bytes:Uint8Array|string)=>createHash('sha256').update(bytes).digest('hex');
const isRecord=(value:unknown):value is Record<string,unknown>=>value!==null&&typeof value==='object'&&!Array.isArray(value);
const isSha=(value:unknown):value is string=>typeof value==='string'&&/^[a-f0-9]{64}$/.test(value);
const isText=(value:unknown):value is string=>typeof value==='string'&&value.trim().length>0;

type CoverageRoute={source:string;route:string;fingerprint:string;closureIds:number[];styles:string[];dependencies:string[]};
type CoverageSource={source:string;fingerprint:string};
type UiCoverage={closurePaths:string[];routes:CoverageRoute[];sources:CoverageSource[]};
type BrowserState={run:string;prefix:string;origin:string;port:number;ready:boolean;diagnostic?:boolean;pid:number;jobsPid:number;jobsPaused?:boolean;productHosts?:boolean;executiveWorkHosts?:boolean;formsHosts?:boolean;sourceFingerprint:string;softwareVersion:string;webEntrypoint:string;jobsEntrypoint:string;release:{id:string;fileManifestSha256:string}};
type UiEvidenceBinding={path:string;sha256:string;bytes:number};

export type WorkflowUiObservationInput={
 routeSource:string;
 targetSource:string;
 checks:readonly string[];
 ownerContext:string|Record<string,unknown>;
 authorityBranch:string;
 authentication:'normal-native-owner'|'anonymous-public-visitor';
};

export type WorkflowUiObservationDocument={
 schema:'erpx-ui-browser-observation/v1';
 result:'passed';
 run:string;
 candidate:string;
 sourceFingerprint:string;
 verificationFingerprint:string;
 releaseId:string;
 releaseManifestSha256:string;
 targetSource:string;
 targetKind:'route'|'source';
 targetFingerprint:string;
 routeSource:string;
 url:string;
 ownerContext:string;
 authorityBranch:string;
 authentication:'normal-native-owner'|'anonymous-public-visitor';
 viewport:{width:number;height:number};
 deviceScaleFactor:1;
 theme:'light'|'dark'|'system';
 resolvedTheme:'light'|'dark';
 checks:string[];
 screenshot:UiEvidenceBinding;
};

export type WorkflowUiObserver={
 observe(label:string):Promise<{screenshot:UiEvidenceBinding;observations:UiEvidenceBinding[]}>;
 stable():Promise<void>;
};

function assert(condition:unknown,code:string):asserts condition{if(!condition)throw new Error(code);}
function safeRepoPath(value:string){
 assert(isText(value)&&value.startsWith('src/')&&!value.includes('\\')&&!value.split('/').some(part=>!part||part==='.'||part==='..'),'UI_WORKFLOW_SOURCE_PATH_REQUIRED');
 return value;
}
function safeRelativePath(value:string){
 assert(isText(value)&&!value.startsWith('/')&&!value.includes('\\')&&!value.split('/').some(part=>!part||part==='.'||part==='..'),'UI_WORKFLOW_RELATIVE_PATH_REQUIRED');
 return value;
}
function ownerContextText(value:WorkflowUiObservationInput['ownerContext']):string{
 if(typeof value==='string'){
  assert(isText(value),'UI_WORKFLOW_OWNER_CONTEXT_REQUIRED');return value;
 }
 assert(isRecord(value)&&Object.keys(value).length>0,'UI_WORKFLOW_OWNER_CONTEXT_REQUIRED');
 const text=JSON.stringify(value);assert(isText(text),'UI_WORKFLOW_OWNER_CONTEXT_REQUIRED');return text;
}
function routeMatches(route:string,url:URL){
 const escape=(value:string)=>value.replace(/[.*+?^${}()|[\]\\]/g,'\\$&');
 const parts=route.split('/').filter(Boolean).map(part=>/^\[\[\.\.\.[^\]]+\]\]$/.test(part)?'(?:/.*)?':/^\[\.\.\.[^\]]+\]$/.test(part)?'/.+':/^\[[^\]]+\]$/.test(part)?'/[^/]+':`/${escape(part)}`).join('')||'/';
 return new RegExp(`^${parts}/?$`).test(url.pathname);
}

/** Pure serialization boundary: it validates explicit metadata and never claims that checks were executed. */
export function serializeWorkflowUiObservation(input:WorkflowUiObservationInput&{
 run:string;candidate:string;sourceFingerprint:string;verificationFingerprint:string;releaseId:string;releaseManifestSha256:string;
 targetSource:string;targetKind:'route'|'source';targetFingerprint:string;url:string;viewport:{width:number;height:number};
 theme:'light'|'dark'|'system';resolvedTheme:'light'|'dark';screenshot:UiEvidenceBinding;hostOrigin?:string;
}):WorkflowUiObservationDocument{
 const routeSource=safeRepoPath(input.routeSource),targetSource=safeRepoPath(input.targetSource),checks=[...input.checks];
 assert(/^[a-f0-9]{12}$/.test(input.run),'UI_WORKFLOW_RUN_REQUIRED');
 assert(/^[a-f0-9]{40}$/.test(input.candidate),'UI_WORKFLOW_CANDIDATE_REQUIRED');
 assert(isSha(input.sourceFingerprint)&&isSha(input.verificationFingerprint)&&isSha(input.releaseManifestSha256)&&isSha(input.targetFingerprint),'UI_WORKFLOW_FINGERPRINT_REQUIRED');
 assert(isText(input.releaseId)&&!input.releaseId.includes('/')&&!input.releaseId.includes('\\'),'UI_WORKFLOW_RELEASE_REQUIRED');
 assert(input.targetKind==='route'||input.targetKind==='source','UI_WORKFLOW_TARGET_KIND_REQUIRED');
 assert(input.authentication==='normal-native-owner'||input.authentication==='anonymous-public-visitor','UI_WORKFLOW_AUTHENTICATION_REQUIRED');
 assert(isText(input.authorityBranch),'UI_WORKFLOW_AUTHORITY_REQUIRED');
 const ownerContext=ownerContextText(input.ownerContext);
 if(input.authentication==='anonymous-public-visitor')assert(ownerContext==='Anonymous public visitor; no ERPX account or authenticated session'&&input.authorityBranch==='published public marketing form','UI_WORKFLOW_PUBLIC_VISITOR_CONTEXT_REQUIRED');
 assert(Array.isArray(input.checks)&&checks.length>0&&checks.every(isText)&&new Set(checks).size===checks.length,'UI_WORKFLOW_CHECKS_REQUIRED');
 assert(input.viewport&&[320,390,768,1024,1440].includes(input.viewport.width)&&Number.isSafeInteger(input.viewport.height)&&input.viewport.height>0&&input.viewport.height<=4096,'UI_WORKFLOW_VIEWPORT_REQUIRED');
 assert(['light','dark','system'].includes(input.theme)&&['light','dark'].includes(input.resolvedTheme),'UI_WORKFLOW_THEME_REQUIRED');
 assert(input.theme==='system'||input.theme===input.resolvedTheme,'UI_WORKFLOW_THEME_MISMATCH');
 const screenshotPath=safeRelativePath(input.screenshot.path);
 assert(isSha(input.screenshot.sha256)&&Number.isSafeInteger(input.screenshot.bytes)&&input.screenshot.bytes>0,'UI_WORKFLOW_SCREENSHOT_BINDING_REQUIRED');
 let url:URL;try{url=new URL(input.url);}catch{throw new Error('UI_WORKFLOW_URL_REQUIRED');}
 const hostOrigin=input.hostOrigin??(input.authentication==='anonymous-public-visitor'?formsHostOrigin:compiledOrigin);
 assert((input.authentication==='anonymous-public-visitor'?hostOrigin===formsHostOrigin:hostOrigin===compiledOrigin||hostOrigin===executiveWorkOrigin)&&url.origin===hostOrigin&&!url.username&&!url.password,'UI_WORKFLOW_COMPILED_ORIGIN_REQUIRED');
 return{
  schema:'erpx-ui-browser-observation/v1',result:'passed',run:input.run,candidate:input.candidate,sourceFingerprint:input.sourceFingerprint,
  verificationFingerprint:input.verificationFingerprint,releaseId:input.releaseId,releaseManifestSha256:input.releaseManifestSha256,
  targetSource:input.targetKind==='route'?routeSource:targetSource,targetKind:input.targetKind,targetFingerprint:input.targetFingerprint,
  routeSource,url:url.href,ownerContext,authorityBranch:input.authorityBranch,authentication:input.authentication,
  viewport:{width:input.viewport.width,height:input.viewport.height},deviceScaleFactor:1,theme:input.theme,resolvedTheme:input.resolvedTheme,
  checks,screenshot:{path:screenshotPath,sha256:input.screenshot.sha256,bytes:input.screenshot.bytes},
 };
}

function readStableFile(root:string,path:string,within:string):Buffer{
 const file=resolve(root,path);assert(file.startsWith(within+sep),'UI_WORKFLOW_FILE_OUTSIDE_BOUNDARY');
 const before=lstatSync(file,{bigint:true});assert(before.isFile()&&realpathSync.native(file)===file&&before.size<=BigInt(128*1024*1024),'UI_WORKFLOW_REGULAR_FILE_REQUIRED');
 const bytes=readFileSync(file),after=lstatSync(file,{bigint:true});
 assert(after.isFile()&&realpathSync.native(file)===file&&before.dev===after.dev&&before.ino===after.ino&&before.size===after.size&&before.mtimeNs===after.mtimeNs&&before.ctimeNs===after.ctimeNs&&BigInt(bytes.length)===after.size,'UI_WORKFLOW_FILE_CHANGED_DURING_READ');
 return bytes;
}

function resolveSource(root:string,path:string){
 const safe=safeRepoPath(path),file=resolve(root,safe);assert(file.startsWith(root+sep),'UI_WORKFLOW_SOURCE_OUTSIDE_ROOT');
 return readStableFile(root,safe,root);
}

function coverageTarget(root:string,bytes:Buffer,input:WorkflowUiObservationInput){
 const coverage=JSON.parse(bytes.toString()) as UiCoverage;
 assert(Array.isArray(coverage.closurePaths)&&Array.isArray(coverage.routes)&&Array.isArray(coverage.sources),'UI_WORKFLOW_COVERAGE_REQUIRED');
 const routes=coverage.routes.filter(row=>row.source===input.routeSource),sources=coverage.sources.filter(row=>row.source===input.targetSource);
 assert(routes.length===1&&sources.length===1,'UI_WORKFLOW_TARGET_INVENTORY_REQUIRED');
 const route=routes[0],source=sources[0];
 assert(isText(route.route)&&isSha(route.fingerprint)&&isSha(source.fingerprint),'UI_WORKFLOW_TARGET_INVENTORY_INVALID');
 assert(Array.isArray(route.dependencies)&&route.dependencies.includes(input.targetSource),'UI_WORKFLOW_ROUTE_TARGET_RELATION_REQUIRED');
 const closureIds=route.closureIds;
 assert(Array.isArray(closureIds)&&closureIds.length>0&&closureIds.every(Number.isSafeInteger)&&new Set(closureIds).size===closureIds.length,'UI_WORKFLOW_ROUTE_CLOSURE_REQUIRED');
 const closure=closureIds.map(id=>coverage.closurePaths[id]);
 assert(closure.every(path=>typeof path==='string')&&closure.includes(input.routeSource),'UI_WORKFLOW_ROUTE_CLOSURE_REQUIRED');
 const styles=route.styles;
 assert(Array.isArray(styles)&&JSON.stringify(styles)===JSON.stringify(closure.filter(path=>path.endsWith('.css'))),'UI_WORKFLOW_ROUTE_STYLES_REQUIRED');
 const routeFingerprint=hash([...closure,...styles].map(path=>`${path}:${hash(resolveSource(root,path))}`).sort().join('\n'));
 const sourceFingerprint=hash(resolveSource(root,input.targetSource));
 assert(routeFingerprint===route.fingerprint&&sourceFingerprint===source.fingerprint,'UI_WORKFLOW_TARGET_SOURCE_CHANGED');
 return{route,routeFingerprint,sourceFingerprint};
}

function validateLabel(label:string){assert(/^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$/.test(label),'UI_WORKFLOW_LABEL_REQUIRED');}
function caseArtifactPath(root:string,path:string,run:string){
 const file=resolve(path),base=resolve(root,browserEvidenceRoot),parts=relative(base,file).split(sep);
 assert(file.startsWith(base+sep)&&parts.length>3&&parts[1]===run&&parts[2]==='artifacts'&&parts.every(part=>part&&part!=='.'&&part!=='..'),'UI_WORKFLOW_CASE_ARTIFACT_REQUIRED');
 assert(realpathSync.native(dirname(file))===dirname(file),'UI_WORKFLOW_CASE_ARTIFACT_DIRECTORY_REQUIRED');
 return file;
}

async function readCohort(root:string,run:string,executiveWorkHostLeasePath?:string,formsHostLeasePath?:string){
 const statePath=resolve(root,'.runtime/browser',run,'state.json'),stateBytes=readStableFile(root,relative(root,statePath),resolve(root,'.runtime/browser',run));
 const state=JSON.parse(stateBytes.toString()) as BrowserState;
 const executiveWorkMode=state.executiveWorkHosts===true;
 const formsHostMode=state.formsHosts===true;
 assert(state.run===run&&state.prefix===`erpx_browser_${run}`&&state.port===3104&&state.ready===true&&!state.diagnostic&&!state.productHosts,'UI_WORKFLOW_COMPILED_COHORT_REQUIRED');
 assert(!(executiveWorkMode&&formsHostMode),'UI_WORKFLOW_HOST_MODE_CONFLICT');
 let hostOrigin=compiledOrigin,executiveWorkLease:ExecutiveWorkHostLease|undefined,formsHostLease:Awaited<ReturnType<typeof readFormsHostLease>>|undefined;
 if(executiveWorkMode){
  assert(!formsHostLeasePath,'UI_WORKFLOW_HOST_MODE_CONFLICT');
  assert(executiveWorkHostLeasePath,'UI_WORKFLOW_EXECUTIVE_WORK_LEASE_REQUIRED');
  const {readExecutiveWorkHostLease}=await import('../../../scripts/testing/executive-work-host-cohort');
  executiveWorkLease=await readExecutiveWorkHostLease(root,executiveWorkHostLeasePath);hostOrigin=executiveWorkOrigin;
 }else if(formsHostMode){
  assert(state.productHosts===false&&state.executiveWorkHosts===false,'UI_WORKFLOW_HOST_MODE_CONFLICT');
  assert(!executiveWorkHostLeasePath&&formsHostLeasePath,'UI_WORKFLOW_FORMS_HOST_LEASE_REQUIRED');
  formsHostLease=await readFormsHostLease(root,formsHostLeasePath,{requireCompleted:false});hostOrigin=formsHostOrigin;
 }else assert(!executiveWorkHostLeasePath&&!formsHostLeasePath&&state.origin===compiledOrigin,'UI_WORKFLOW_COMPILED_COHORT_REQUIRED');
 assert(Boolean(executiveWorkLease)===executiveWorkMode&&state.origin===compiledOrigin,'UI_WORKFLOW_HOST_MODE_REQUIRED');
 if(executiveWorkLease)assert(executiveWorkLease.run===run&&executiveWorkLease.sourceFingerprint===state.sourceFingerprint&&executiveWorkLease.release.id===state.release?.id&&executiveWorkLease.release.fileManifestSha256===state.release?.fileManifestSha256&&executiveWorkLease.ui.backend.pid===state.pid&&executiveWorkLease.ui.backend.jobsPid===state.jobsPid,'UI_WORKFLOW_EXECUTIVE_WORK_LEASE_BINDING_REQUIRED');
 assert(Boolean(formsHostLease)===formsHostMode,'UI_WORKFLOW_FORMS_HOST_MODE_REQUIRED');
 if(formsHostLease)assert(formsHostLease.run===run&&formsHostLease.sourceFingerprint===state.sourceFingerprint&&formsHostLease.release.id===state.release?.id&&formsHostLease.release.fileManifestSha256===state.release?.fileManifestSha256&&formsHostLease.ui.backend.pid===state.pid&&formsHostLease.ui.backend.jobsPid===state.jobsPid&&formsHostLease.ui.backend.state.path===statePath&&formsHostLease.ui.backend.state.sha256===hash(stateBytes)&&formsHostLease.ui.backend.state.bytes===stateBytes.length,'UI_WORKFLOW_FORMS_HOST_LEASE_BINDING_REQUIRED');
 assert(Number.isSafeInteger(state.pid)&&state.pid>0&&Number.isSafeInteger(state.jobsPid)&&state.jobsPid>0&&state.jobsPaused!==true,'UI_WORKFLOW_COMPILED_PROCESSES_REQUIRED');
 assert(state.webEntrypoint===resolve(root,'.runtime/browser',run,'release/server.js')&&state.jobsEntrypoint===resolve(root,'.runtime/browser',run,'release/tools/jobs.mjs'),'UI_WORKFLOW_COMPILED_ENTRYPOINT_REQUIRED');
 assert(state.release&&isText(state.release.id)&&!state.release.id.includes('/')&&!state.release.id.includes('\\')&&isSha(state.release.fileManifestSha256),'UI_WORKFLOW_RELEASE_REQUIRED');
 const releaseDirectory=resolve(root,'.runtime/releases',state.release.id);
 assert(realpathSync.native(releaseDirectory)===releaseDirectory,'UI_WORKFLOW_RELEASE_DIRECTORY_REQUIRED');
 const releaseBytes=readStableFile(root,relative(root,resolve(releaseDirectory,'release.json')),releaseDirectory),manifestBytes=readStableFile(root,relative(root,resolve(releaseDirectory,'release-files.json')),releaseDirectory);
 const release=JSON.parse(releaseBytes.toString()) as {id:string;softwareVersion:string;sourceFingerprint:string;node:string;postgres:string;fileManifestSha256:string};
 assert(release.id===state.release.id&&release.sourceFingerprint===state.sourceFingerprint&&release.softwareVersion===state.softwareVersion&&release.node==='26.11.1'&&release.postgres==='18.6'&&release.fileManifestSha256===state.release.fileManifestSha256&&hash(manifestBytes)===release.fileManifestSha256,'UI_WORKFLOW_RELEASE_IDENTITY_REQUIRED');
 const manifest=JSON.parse(manifestBytes.toString()) as Record<string,unknown>;
 assert(isRecord(manifest)&&Object.keys(manifest).length>0&&Object.entries(manifest).every(([path,digest])=>safeRelativePath(path)&&isSha(digest)),'UI_WORKFLOW_RELEASE_MANIFEST_REQUIRED');
 for(const [entrypoint,relativePath] of [[state.webEntrypoint,'server.js'],[state.jobsEntrypoint,'tools/jobs.mjs']] as const){
  const expected=manifest[relativePath];assert(isSha(expected),'UI_WORKFLOW_RELEASE_ENTRYPOINT_MANIFEST_REQUIRED');
  const bytes=readStableFile(root,relative(root,resolve(entrypoint)),resolve(root,'.runtime/browser',run,'release'));
  assert(hash(bytes)===expected,'UI_WORKFLOW_COMPILED_ENTRYPOINT_CHANGED');
 }
 return{statePath,stateBytes,state,releaseDirectory,releaseBytes,manifestBytes,hostOrigin};
}

async function currentIdentity(root:string,state:BrowserState){
 const sourceFingerprint=await runtimeSourceFingerprint(root),verificationFingerprint=await verificationSourceFingerprint(root,sourceFingerprint);
 assert(state.sourceFingerprint===sourceFingerprint&&state.softwareVersion===`release-${sourceFingerprint}`,'UI_WORKFLOW_RUNTIME_FINGERPRINT_MISMATCH');
 const candidate=execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim();
 assert(/^[a-f0-9]{40}$/.test(candidate),'UI_WORKFLOW_CANDIDATE_REQUIRED');
 return{sourceFingerprint,verificationFingerprint,candidate};
}

/** Evidence plumbing only. The caller must perform and pass the actual workflow assertions before capture. */
export async function createWorkflowUiObserver(page:Page,info:TestInfo,input:WorkflowUiObservationInput):Promise<WorkflowUiObserver>{
 const root=resolve(process.cwd());assert(realpathSync.native(root)===root,'UI_WORKFLOW_ROOT_REQUIRED');
 safeRepoPath(input.routeSource);safeRepoPath(input.targetSource);
 assert((input.authentication==='normal-native-owner'||input.authentication==='anonymous-public-visitor')&&isText(input.authorityBranch),'UI_WORKFLOW_OWNER_AUTHORITY_REQUIRED');
 if(input.authentication==='anonymous-public-visitor')assert(input.ownerContext==='Anonymous public visitor; no ERPX account or authenticated session'&&input.authorityBranch==='published public marketing form','UI_WORKFLOW_PUBLIC_VISITOR_CONTEXT_REQUIRED');
 assert(Array.isArray(input.checks)&&input.checks.length>0&&input.checks.every(isText)&&new Set(input.checks).size===input.checks.length,'UI_WORKFLOW_CHECKS_REQUIRED');
 const environmentRun=process.env.ERPX_BROWSER_SIMULATION_RUN??process.env.ERPX_DEMO_RUN;
 assert(environmentRun&&/^[a-f0-9]{12}$/.test(environmentRun),'UI_WORKFLOW_RUN_REQUIRED');
 if(process.env.ERPX_BROWSER_SIMULATION_RUN&&process.env.ERPX_DEMO_RUN)assert(process.env.ERPX_BROWSER_SIMULATION_RUN===process.env.ERPX_DEMO_RUN,'UI_WORKFLOW_RUN_MISMATCH');
 const run=environmentRun,executiveWorkHostLeasePath=process.env.ERPX_EXECUTIVE_WORK_HOST_LEASE,formsHostLeasePath=process.env.ERPX_FORMS_HOST_LEASE,cohort=await readCohort(root,run,executiveWorkHostLeasePath,formsHostLeasePath),identity=await currentIdentity(root,cohort.state);
 async function assertAnonymousContext(){if(input.authentication==='anonymous-public-visitor')assert((await page.context().cookies()).length===0,'UI_WORKFLOW_PUBLIC_VISITOR_COOKIES_REQUIRED');}
 await assertAnonymousContext();
 const coverageBytes=readStableFile(root,coveragePath,root),targets=coverageTarget(root,coverageBytes,input);
 ownerContextText(input.ownerContext);
 const artifactBase=resolve(root,browserEvidenceRoot),created:{path:string;within:string;sha256:string;bytes:number}[]=[];

 async function stable(){
  assert(created.length>0,'UI_WORKFLOW_OBSERVATION_REQUIRED');
  assert(readStableFile(root,relative(root,cohort.statePath),dirname(cohort.statePath)).equals(cohort.stateBytes),'UI_WORKFLOW_STATE_CHANGED');
  assert(readStableFile(root,coveragePath,root).equals(coverageBytes),'UI_WORKFLOW_COVERAGE_CHANGED');
  assert(readStableFile(root,relative(root,resolve(cohort.releaseDirectory,'release.json')),cohort.releaseDirectory).equals(cohort.releaseBytes),'UI_WORKFLOW_RELEASE_CHANGED');
  assert(readStableFile(root,relative(root,resolve(cohort.releaseDirectory,'release-files.json')),cohort.releaseDirectory).equals(cohort.manifestBytes),'UI_WORKFLOW_RELEASE_MANIFEST_CHANGED');
  await assertAnonymousContext();
  const currentCohort=await readCohort(root,run,executiveWorkHostLeasePath,formsHostLeasePath),current=await currentIdentity(root,currentCohort.state);
  assert(current.sourceFingerprint===identity.sourceFingerprint&&current.verificationFingerprint===identity.verificationFingerprint&&current.candidate===identity.candidate&&currentCohort.hostOrigin===cohort.hostOrigin,'UI_WORKFLOW_SOURCE_CHANGED');
  const now=coverageTarget(root,readStableFile(root,coveragePath,root),input);
  assert(now.routeFingerprint===targets.routeFingerprint&&now.sourceFingerprint===targets.sourceFingerprint,'UI_WORKFLOW_TARGET_CHANGED');
  for(const item of created){const bytes=readStableFile(root,item.path,item.within);assert(bytes.length===item.bytes&&hash(bytes)===item.sha256,'UI_WORKFLOW_OUTPUT_CHANGED');}
 }

 return{
  async observe(label:string){
   validateLabel(label);
   await assertAnonymousContext();
   assert(!page.isClosed(),'UI_WORKFLOW_PAGE_REQUIRED');
   const observedUrl=page.url();let url:URL;try{url=new URL(observedUrl);}catch{throw new Error('UI_WORKFLOW_URL_REQUIRED');}
   assert(url.origin===cohort.hostOrigin&&!url.username&&!url.password&&routeMatches(targets.route.route,url),'UI_WORKFLOW_ROUTE_REQUIRED');
   const viewport=page.viewportSize();assert(viewport&&[320,390,768,1024,1440].includes(viewport.width)&&Number.isSafeInteger(viewport.height)&&viewport.height>0,'UI_WORKFLOW_VIEWPORT_REQUIRED');
   assert(await page.evaluate(()=>devicePixelRatio)===1,'UI_WORKFLOW_DEVICE_SCALE_REQUIRED');
   const theme=await page.locator('html').getAttribute('data-theme'),resolvedTheme=await page.locator('html').evaluate(node=>getComputedStyle(node).colorScheme);
   assert(theme==='light'||theme==='dark'||theme==='system','UI_WORKFLOW_THEME_REQUIRED');
   assert((resolvedTheme==='light'||resolvedTheme==='dark')&&(theme==='system'||theme===resolvedTheme),'UI_WORKFLOW_THEME_MISMATCH');
   const imagePath=caseArtifactPath(root,info.outputPath(`${label}.png`),run);assert(!existsSync(imagePath),'UI_WORKFLOW_SCREENSHOT_PATH_EXISTS');
   const image=await page.screenshot({path:imagePath,type:'png',fullPage:false,scale:'css',animations:'disabled'});
   assert(image.length>=45&&image.length<=64*1024*1024&&image.subarray(0,8).equals(Buffer.from([137,80,78,71,13,10,26,10]))&&image.readUInt32BE(16)===viewport.width&&image.readUInt32BE(20)===viewport.height,'UI_WORKFLOW_SCREENSHOT_INVALID');
   await info.attach('workflow-ui-screenshot',{path:imagePath,contentType:'image/png'});
   const screenshotAttachment=info.attachments.at(-1);
   assert(screenshotAttachment?.name==='workflow-ui-screenshot'&&screenshotAttachment.contentType==='image/png'&&screenshotAttachment.path,'UI_WORKFLOW_SCREENSHOT_ATTACHMENT_REQUIRED');
   const retainedPath=caseArtifactPath(root,screenshotAttachment.path,run),caseParts=relative(artifactBase,retainedPath).split(sep),artifactsDirectory=resolve(artifactBase,caseParts[0],caseParts[1],'artifacts'),retainedBytes=readStableFile(root,relative(root,retainedPath),artifactsDirectory);
   assert(retainedBytes.equals(image),'UI_WORKFLOW_SCREENSHOT_ATTACHMENT_CHANGED');
   const screenshot={path:relative(root,retainedPath).replaceAll('\\','/'),sha256:hash(retainedBytes),bytes:retainedBytes.length};
   const viewportBinding={width:viewport.width,height:viewport.height};
   const outputObservations:UiEvidenceBinding[]=[];
   for(const target of [
    {kind:'route' as const,source:input.routeSource,fingerprint:targets.routeFingerprint},
    {kind:'source' as const,source:input.targetSource,fingerprint:targets.sourceFingerprint},
   ]){
    const document=serializeWorkflowUiObservation({
     ...input,run,candidate:identity.candidate,sourceFingerprint:identity.sourceFingerprint,verificationFingerprint:identity.verificationFingerprint,
     releaseId:cohort.state.release.id,releaseManifestSha256:cohort.state.release.fileManifestSha256,targetSource:target.source,targetKind:target.kind,
     targetFingerprint:target.fingerprint,url:observedUrl,viewport:viewportBinding,theme,resolvedTheme,screenshot,hostOrigin:cohort.hostOrigin,
    });
    const outputPath=caseArtifactPath(root,info.outputPath(`${label}-${target.kind}.json`),run);assert(!existsSync(outputPath),'UI_WORKFLOW_OBSERVATION_PATH_EXISTS');
    const bytes=Buffer.from(JSON.stringify(document,null,2)+'\n');writeFileSync(outputPath,bytes,{flag:'wx',mode:0o600});
    await info.attach('erpx-ui-observation',{path:outputPath,contentType:'application/json'});
    const attachment=info.attachments.at(-1);
    assert(attachment?.name==='erpx-ui-observation'&&attachment.contentType==='application/json'&&attachment.path,'UI_WORKFLOW_OBSERVATION_ATTACHMENT_REQUIRED');
    const retainedObservation=caseArtifactPath(root,attachment.path,run),actual=readStableFile(root,relative(root,retainedObservation),artifactsDirectory);
    assert(actual.equals(bytes),'UI_WORKFLOW_OBSERVATION_ATTACHMENT_CHANGED');
    const binding={path:relative(root,retainedObservation).replaceAll('\\','/'),sha256:hash(actual),bytes:actual.length};
    outputObservations.push(binding);created.push({path:relative(root,retainedObservation),within:artifactsDirectory,sha256:binding.sha256,bytes:binding.bytes});
   }
   created.push({path:relative(root,retainedPath),within:artifactsDirectory,sha256:screenshot.sha256,bytes:screenshot.bytes});
   return{screenshot,observations:outputObservations};
  },
  stable,
 };
}
