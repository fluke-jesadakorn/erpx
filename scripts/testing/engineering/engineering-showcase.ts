/** Package only a revalidated complete engineering run. Never publishes or deploys. */
import {createHash} from 'node:crypto';
import {chmod,mkdir,readFile,realpath,stat,writeFile} from 'node:fs/promises';
import {join,relative,resolve,sep} from 'node:path';
import registry from "@erpx/catalogue/module-registry.json";
import {runtimeSourceFingerprint,verificationSourceFingerprint} from "../../local/source-fingerprint";
import {playwrightObservations,vitestObservations} from "./engineering-test-results";
import {generateEngineeringWalkthrough} from "./engineering-walkthrough-video";
import {verifyEngineeringCandidateInventory} from "./engineering-candidate-inventory";

const root=resolve(import.meta.dir,'../../..'),requested=process.argv[2];
if(!requested)throw new Error('EXACT_COMPLETE_ENGINEERING_RUN_REQUIRED');
const directory=await realpath(resolve(root,requested)),allowed=join(root,'.scratch/dev/engineering-completion/runs')+sep;
if(!directory.startsWith(allowed))throw new Error('PRIVATE_ENGINEERING_RUN_REQUIRED');
const node=join(root,'.runtime/node/node-v26.11.1-darwin-arm64/bin/node');
if(Bun.version!=='1.4.3'||Bun.spawnSync([node,'--version']).stdout.toString().trim()!=='v26.11.1')throw new Error('PINNED_TOOLCHAIN_REQUIRED');
const checking=Bun.spawn([process.execPath,'scripts/testing/engineering/engineering-completion.ts','check',directory],{cwd:root,stdout:'pipe',stderr:'pipe'});
const [checkOutput,checkError,checkExit]=await Promise.all([new Response(checking.stdout).text(),new Response(checking.stderr).text(),checking.exited]);
if(checkExit!==0)throw new Error('ALL_ENGINEERING_GATES_MUST_PASS_BEFORE_PACKAGING');
// Child errors are intentionally retained only in the private evidence directory.
await writeFile(join(directory,'showcase-reassessment.log'),checkOutput+checkError,{mode:0o600});
const completion=JSON.parse(await readFile(join(directory,'completion.json'),'utf8')),candidate=JSON.parse(await readFile(join(directory,'compiled-candidate.json'),'utf8'));
const sourceFingerprint=await runtimeSourceFingerprint(root),verificationFingerprint=await verificationSourceFingerprint(root,sourceFingerprint);
if(!completion.complete||completion.passedModules!==33||completion.passedCapabilities!==187||completion.sourceFingerprint!==sourceFingerprint||completion.verificationFingerprint!==verificationFingerprint||candidate.sourceFingerprint!==sourceFingerprint)throw new Error('COMPLETE_SAME_SOURCE_BUILD_EVIDENCE_REQUIRED');
const destination=join(directory,'showcase');await mkdir(destination,{recursive:true,mode:0o700});
const hash=(bytes:Buffer|string)=>createHash('sha256').update(bytes).digest('hex');
const relativeEvidence=(name:string)=>`../${name}`;
const manifest=JSON.parse(await readFile(join(root,'tests/catalogue/engineering-scenarios.json'),'utf8'));
const browser=JSON.parse(await readFile(join(directory,'browser.json'),'utf8')),performance=JSON.parse(await readFile(join(directory,'performance.json'),'utf8'));
const artifacts:Array<{name:string;sha256:string;bytes:number}>=[];
async function artifact(name:string,content:string|Buffer){const path=join(destination,name);await mkdir(resolve(path,'..'),{recursive:true,mode:0o700});await writeFile(path,content,{mode:0o600});await chmod(path,0o600);artifacts.push({name,sha256:hash(content),bytes:Buffer.byteLength(content)});}
const observations=[...vitestObservations(root,JSON.parse(await readFile(join(directory,'vitest.json'),'utf8'))),...playwrightObservations(root,browser)];
const screenshotLinks=new Map<string,string>();
for(const observation of observations)for(const path of observation.artifacts??[])if(path.endsWith('.png')&&!screenshotLinks.has(path)){
 const source=await realpath(path);if(!source.startsWith(directory+sep))throw new Error('SCREENSHOT_OUTSIDE_VERIFIED_EVIDENCE');
 const bytes=await readFile(source),name=`screenshots/${hash(bytes)}.png`;if(!artifacts.some(item=>item.name===name))await artifact(name,bytes);screenshotLinks.set(path,name);
}
const capabilityReport=['# ERPX — 187 verified capabilities','',`Runtime source: ${sourceFingerprint}`,`Verification source: ${verificationFingerprint}`,`Compiled candidate: ${candidate.softwareVersion}`,`Compiled files SHA256: ${candidate.fileManifestSha256}`,'','Environment: isolated local PostgreSQL, normal fictional multi-persona authentication, real local scanning and compiled web/jobs. Actual provider sandboxes and declared missing-provider adapters are identified by each case. External business/provider certification remains separate.',''];
for(const capability of manifest.capabilities){
 capabilityReport.push(`## ${capability.id}`,'','Result: passed.','',`Workflow: ${capability.workflow.join(' → ')}`,'',...capability.expectedResults.map((result:string)=>`- ${result}`),'','Executed proof:','');
 for(const required of capability.cases){
  const matching=observations.filter(item=>item.runner===required.runner&&item.file===required.file&&item.title===required.title&&item.project===required.project);
  if(matching.length!==1||matching[0].status!=='passed')throw new Error('REQUIRED_CAPABILITY_EXECUTION_MISSING_DURING_PACKAGING');
  const observed=matching[0],screenshots=(observed.artifacts??[]).filter(path=>screenshotLinks.has(path));
  capabilityReport.push(`- Passed ${required.runner}${required.project?` / ${required.project}`:''}: \`${required.file}\` — ${required.title} (${(observed.durationMs??0).toFixed(0)} ms).`, `  Proof: ${required.proof}`);
  if(screenshots.length)capabilityReport.push(`  ${screenshots.map((path,index)=>`[Screenshot ${index+1}](${screenshotLinks.get(path)})`).join(' · ')}`);
 }
 capabilityReport.push('');
}
await artifact('187-capabilities.md',capabilityReport.join('\n'));
const rows=registry.map(module=>{
 const capabilities=manifest.capabilities.filter((item:{moduleId:string})=>item.moduleId===module.id);
 const cases=[...new Map(capabilities.flatMap((item:{cases:Array<{runner:string;file:string;title:string;project?:string}>})=>item.cases).filter((item:{runner:string})=>item.runner==='playwright').map((item:{file:string;title:string;project?:string})=>[`${item.file}\0${item.title}\0${item.project}`,item])).values()] as Array<{file:string;title:string;project?:string}>;
 return {id:module.id,name:module.name,capabilities:capabilities.length,browserCases:cases};
});
if(rows.length!==33||rows.reduce((sum,row)=>sum+row.capabilities,0)!==187)throw new Error('EXACT_SHOWCASE_CATALOGUE_REQUIRED');
const guideRun=browser.cohorts.find((cohort:{project:string})=>cohort.project==='mobile')?.run;
if(!guideRun||!/^[a-f0-9]{12}$/.test(guideRun))throw new Error('VERIFIED_SHOWCASE_COHORT_REQUIRED');
const nativeGuide=JSON.parse(await readFile(join(root,'.runtime/browser',guideRun,'engineering-showcase-guide.json'),'utf8'));
if(nativeGuide.run!==guideRun||nativeGuide.origin!=='http://127.0.0.1:3104'||nativeGuide.kind!=='native-fictional-inventory'||nativeGuide.moduleCount!==33||nativeGuide.sourceFingerprint!==sourceFingerprint||nativeGuide.modules?.length!==33||new Set(nativeGuide.modules.map((item:{id:string})=>item.id)).size!==33||registry.some(module=>!nativeGuide.modules.some((item:{id:string})=>item.id===module.id)))throw new Error('COMPLETE_NATIVE_SHOWCASE_GUIDE_REQUIRED');
await artifact('native-examples.json',JSON.stringify(nativeGuide,null,2)+'\n');
await artifact('33-modules.json',JSON.stringify({sourceFingerprint,verificationFingerprint,softwareVersion:candidate.softwareVersion,modules:rows},null,2)+'\n');
await artifact('quickstart.md',[
 '# ERPX local engineering showcase','',
 `Verified ${candidate.softwareVersion}; source ${sourceFingerprint}. All 33 modules and 187 capabilities passed the required same-build engineering gates.`,
 '', 'Use Node 26.11.1, Bun 1.4.3 and the installed loopback PostgreSQL service. This package does not deploy an environment.',
 '', 'Before preparing a run, configure the isolated PostgreSQL 18.6 test administrator on 127.0.0.1:5432. The required role is erpx_test_admin with CREATEDB, without SUPERUSER or CREATEROLE, and without CONNECT authority to the other projects. Store its local password only in the private, mode-0600 .env.test.local file or process environment:',
 '', '```dotenv', 'ERPX_TEST_ADMIN_URL="postgresql://erpx_test_admin:<local-test-password>@127.0.0.1:5432/postgres"', '```',
 '', 'prepare creates its own private databases.json: exactly CONTROL_DB, CONTROL_READ_DB and H001/H002/H003 write/read bindings, all named erpx_browser_<run>_*, with three local file resource slots. The launcher checks the loopback admin role, exact run and database ownership before creating or dropping these resources.',
 '', 'The attachment journeys require the installed local ClamAV 1.5.4 Unix-socket service and definitions refreshed within 24 hours, plus libheif 1.23.5 and Poppler 26.09.0. The pinned executable paths are recorded in ops/macos/clamav/runtime-versions.json. This isolated launcher expects .scratch/dev/clamav/clamd.sock and .scratch/dev/clamav/db, /opt/homebrew/opt/libheif/bin/heif-convert and /opt/homebrew/bin/pdftoppm. Start and verify that local scanner and preview runtime before prepare/start; the launcher verifies readiness and scans the retained originals before it opens the isolated upload gate.',
 '', '```sh',
 'export PATH="$PWD/.runtime/bun-1.4.3/bun-darwin-aarch64:$PWD/.runtime/node/node-v26.11.1-darwin-arm64/bin:$PATH"',
 'export ERPX_NODE_BIN="$PWD/.runtime/node/node-v26.11.1-darwin-arm64/bin/node"',
 '"$ERPX_NODE_BIN" --version # v26.11.1',
 'bun --version # 1.4.3',
 'bun install --frozen-lockfile',
 '# Generate the ignored Next declarations before the isolated builder copies them.',
 '"$ERPX_NODE_BIN" node_modules/next/dist/bin/next typegen',
 '# Rebuild the retained source with the pinned tools, or extract candidate.tar.gz',
 '# into an empty directory under .runtime/releases/ and use that release path.',
 'bun run deploy:build',
 'export ERPX_BROWSER_SIMULATION_PORT=3104',
 'export ERPX_BROWSER_SIMULATION_ATTACHMENTS=local',
 'export ERPX_BROWSER_SIMULATION_JOBS=1',
 'export ERPX_BROWSER_SIMULATION_LEGAL=1',
 'bun scripts/local/run-server-script.ts scripts/local/browser-simulation.ts prepare',
 '# Use the exact run returned above; start refuses another run, source or occupied port.',
 'ERPX_BROWSER_SIMULATION_RELEASE_DIR="$PWD/.runtime/releases/latest" bun scripts/local/run-server-script.ts scripts/local/browser-simulation.ts start <run>',
 '```','',
 'Open http://127.0.0.1:3104 and sign in normally with the fictional persona for the workflow. Private credential files under `.runtime/browser/<run>/` contain generated passwords; they are not part of this shareable package.',
 '', 'Every fresh run produces `engineering-showcase-guide.json`: all 33 module paths, native source IDs, fictional persona labels and initial workflow steps. This verified cohort is retained in [native-examples.json](native-examples.json). The base company and Hospitality guides are `demo-guide.json` and `hospitality-guide.json`. Additional native source IDs and personas are in `engineering-money-people.json`, `engineering-finance-close.json`, `engineering-thai-payroll.json`, `workplace-platform-guide.json`, `workplace-platform-credentials.json`, `legal-guide.json`, and `legal-credentials.json`. Two-period Finance close uses the separately labeled Finance close Companies and preparer/approver assignments; their period IDs are whitelisted in sourceReferences.financeClose. `state.json` contains the Business retail, production, project, field service, ecommerce and scoped Customer Authority sources. Choose the matching published assignment through the normal account switcher.',
 '', 'Reset a running showcase in two phases: stop its exact owned web/jobs process groups, wait for them to exit, then repeat destroy to drop its eight disposable databases and remove its private run directory. If the first invocation already reports destroyed:true, that run is already fully removed. Prepare a fresh run after the drop phase completes:',
 '', '```sh', 'export ERPX_BROWSER_SIMULATION_PORT=3104', '# Phase 1: stop the exact running cohort.', 'bun scripts/local/run-server-script.ts scripts/local/browser-simulation.ts destroy <run>', '# Wait for the owned processes to exit, then Phase 2: drop that same run.', 'bun scripts/local/run-server-script.ts scripts/local/browser-simulation.ts destroy <run>', '# The new preparation returns new source IDs and private fictional credentials.', 'bun scripts/local/run-server-script.ts scripts/local/browser-simulation.ts prepare','```','',
 'Development remains on 3100, private production on 3101 and shared PostgreSQL on 5432. The launcher never uses staging or reserved ports.',
 '', 'Declared no-provider adapters cover absent directory, advertising, academy, bank and AI accounts. Actual Stripe Sandbox evidence and official dated Thai rules are identified in the verified reports. External business/provider/device certification remains separate.',
 '', `Required case details: [187 capabilities](187-capabilities.md). Financial, stock, files, workers, Legal26 and recovery evidence: [gate report](${relativeEvidence('completion.md')}).`,
 '', '| Module | Capabilities | Required browser workflow |','| --- | ---: | --- |',
 ...rows.map(row=>`| ${row.name} | ${row.capabilities} | ${row.browserCases.length?row.browserCases.map(item=>`${item.file}: ${item.title}`).filter((item,index,list)=>list.indexOf(item)===index).join('<br>'):'Use the native scenario steps and results in 187-capabilities.md; full integration and owner evidence are retained.'} |`),'',
 ].join('\n'));
await artifact('architecture.md',[
 '# ERPX architecture','',
 'The pinned Next.js application exposes `/api/v1` commands owned by domain services. Normal Better Auth sessions select published IAM assignments; every owner enforces current Company and source scope. Commands use expected versions, idempotency receipts and PostgreSQL transactions containing audit and outbox writes.',
 '', '```mermaid','flowchart LR',
 '  Browser["Console / public form / scoped portal"] --> Auth["Better Auth + current IAM assignment"]',
 '  Auth --> Owner["Domain owner commands /api/v1"]',
 '  Owner --> DB["PostgreSQL: records, receipts, audit, outbox"]',
 '  Owner --> Money["Immutable integer money + dated FX / rule revisions"]',
 '  Owner --> Files["Shared File Sources: owner, version, scan, quota"]',
 '  DB --> Jobs["Durable compiled jobs: retries, delivery, projections"]',
 '  Jobs --> Adapters["Declared adapters / actual provider sandbox"]',
 '  DB --> Evidence["Same-build integration, browser, recovery, performance"]',
 '```','',
 'Native Sales/Procurement/Inventory/Manufacturing/Field/POS/PMS sources hand off to Finance without replacing their owner histories. Money reversals retain original FX and cumulative rounding; allocated carries conserve source minor units. Payroll retains effective rule digests and independently reviewed employee declarations. Issued reports and employee slips retain immutable revisions. Shared sources are rechecked before AI or portal use.',
 '', 'The isolated candidate uses eight disposable databases, three resource slots, private local files, actual ClamAV and compiled web/jobs on 3104. Additive upgrade, encrypted eight-database restore and compatible/incompatible rollback checks use the identical candidate manifest.',
 '', `Runtime source: ${sourceFingerprint}. Verification source: ${verificationFingerprint}. Compiled files manifest: ${candidate.fileManifestSha256}.`,'',
 ].join('\n'));
await artifact('performance.md',['# ERPX verified local performance','',`Environment: ${performance.environment}. Duration: ${(performance.durationMs/60000).toFixed(2)} minutes; distinct accounts: ${performance.accountIds.length}.`, '', '```json',JSON.stringify(performance.metrics,null,2),'```','',`Process continuity: ${performance.processes.sameProcesses}; runtime errors: ${JSON.stringify(performance.runtimeErrors)}. Raw samples SHA256: ${performance.samplesSha256}.`, '',`[Raw evidence](${relativeEvidence('performance.json')})`,''].join('\n'));
const clips:Array<{title:string;project:string;path:string;sha256:string;bytes:number}>=[];
function visit(suites:Array<{title?:string;suites?:unknown[];specs?:Array<{title:string;tests:Array<{projectName:string;results:Array<{status:string;attachments?:Array<{name:string;path?:string;contentType:string}>}>}>}>}>,ancestors:string[]=[]){
 for(const suite of suites){const prefix=[...ancestors,...(suite.title&&!/\.spec\.ts$/.test(suite.title)?[suite.title]:[])];for(const spec of suite.specs??[])for(const result of spec.tests)for(const run of result.results)if(run.status==='passed')for(const attachment of run.attachments??[])if(attachment.contentType==='video/webm'&&attachment.path)clips.push({title:[...prefix,spec.title].join(' '),project:result.projectName,path:attachment.path,sha256:'',bytes:0});if(suite.suites)visit(suite.suites as Parameters<typeof visit>[0],prefix);}
}
for(const report of browser.reports)visit(report.suites);
for(const [index,clip] of clips.entries()){
 const source=await realpath(clip.path);if(!source.startsWith(directory+sep))throw new Error('WALKTHROUGH_VIDEO_OUTSIDE_VERIFIED_EVIDENCE');
 const bytes=await readFile(source),name=`videos/${String(index+1).padStart(3,'0')}-${clip.project}.webm`;await artifact(name,bytes);clip.path=name;clip.sha256=hash(bytes);clip.bytes=bytes.length;
}
if(!clips.length)throw new Error('VERIFIED_BROWSER_WALKTHROUGH_VIDEO_REQUIRED');
await artifact('walkthrough-videos.json',JSON.stringify({sourceFingerprint,softwareVersion:candidate.softwareVersion,clips},null,2)+'\n');
const walkthrough=await generateEngineeringWalkthrough({directory:destination,clips,identity:{sourceFingerprint,softwareVersion:candidate.softwareVersion,engineeringComplete:true}});
for(const name of ['walkthrough.mp4','walkthrough-chapters.json','walkthrough-opening.png','walkthrough-workflow.png']){const bytes=await readFile(join(destination,name));artifacts.push({name,sha256:hash(bytes),bytes:bytes.length});}
await artifact('walkthrough.html',`<!doctype html><html lang="en"><meta charset="utf-8"><title>ERPX verified walkthrough</title><style>body{font:16px system-ui;margin:3rem auto;max-width:1000px;padding:0 1rem}video{width:100%;max-height:680px;background:#171717}article{margin:3rem 0}h1{font-size:2rem}small{word-break:break-all}</style><h1>ERPX — actual verified browser workflows</h1><p>33 modules / 187 capabilities. Fictional accounts; sandbox or declared adapters. Engineering evidence; external certification remains separate.</p><small>${sourceFingerprint}</small><h2>Walkthrough · ${(walkthrough.durationSeconds/60).toFixed(1)} minutes</h2><video controls preload="metadata" src="walkthrough.mp4" poster="walkthrough-opening.png"></video><p>Actual browser excerpts; [<a href="walkthrough-chapters.json">chapter/source evidence</a>]. Original complete recordings follow.</p>${clips.map(clip=>`<article><h2>${clip.title.replaceAll('&','&amp;').replaceAll('<','&lt;')} · ${clip.project}</h2><video controls preload="metadata" src="${clip.path}"></video><p>${clip.sha256}</p></article>`).join('')}</html>`);
// Archive the exact reviewable working tree, including uncommitted source.
// Git's ignore rules exclude dotenv credentials, installed dependencies,
// private runtime data, previous releases and the evidence package itself.
const listing=Bun.spawn(['git','ls-files','--cached','--others','--exclude-standard','-z'],{cwd:root,stdout:'pipe',stderr:'pipe'}),listed=await new Response(listing.stdout).text();
if(await listing.exited!==0)throw new Error('REPRODUCIBLE_SOURCE_INVENTORY_REQUIRED');
const sourceFiles:string[]=[];
for(const name of [...new Set(listed.split('\0').filter(Boolean))].sort()){
 if(name.startsWith('/')||name.split('/').includes('..'))throw new Error('UNSAFE_PATH_IN_SOURCE_PACKAGE');
 if(/(^|\/)(\.env(?:\..*)?|node_modules|\.runtime|\.scratch|\.next)(\/|$)/.test(name)&&name!=='.env.example')continue;
 try{if((await stat(join(root,name))).isFile())sourceFiles.push(name);}catch(error){if((error as NodeJS.ErrnoException).code!=='ENOENT')throw error;}
}
const inventory=join(destination,'source-inventory.txt');await writeFile(inventory,sourceFiles.join('\0')+'\0',{mode:0o600});
const archive=join(destination,'source.tar.gz'),archiver=Bun.spawn(['tar','--null','-czf',archive,'-T',inventory],{cwd:root,stdout:'pipe',stderr:'pipe'});
const archiveError=await new Response(archiver.stderr).text();if(await archiver.exited!==0){await writeFile(join(directory,'showcase-archive-error.log'),archiveError,{mode:0o600});throw new Error('REPRODUCIBLE_SOURCE_ARCHIVE_FAILED');}
await chmod(archive,0o600);const sourceArchive=await readFile(archive);artifacts.push({name:'source.tar.gz',sha256:hash(sourceArchive),bytes:sourceArchive.length});
await artifact('source-inventory.json',JSON.stringify({sourceFingerprint,verificationFingerprint,fileCount:sourceFiles.length,files:sourceFiles},null,2)+'\n');
const releaseDirectory=await realpath(resolve(root,candidate.releaseDirectory));
if(!releaseDirectory.startsWith(join(root,'.runtime/releases')+sep))throw new Error('VERIFIED_CANDIDATE_DIRECTORY_REQUIRED');
const candidateInventory=await verifyEngineeringCandidateInventory(releaseDirectory,candidate.fileManifestSha256);
await artifact('candidate-inventory.json',JSON.stringify({sourceFingerprint,softwareVersion:candidate.softwareVersion,...candidateInventory},null,2)+'\n');
const candidateArchive=join(destination,'candidate.tar.gz'),candidateArchiver=Bun.spawn(['tar','-czf',candidateArchive,'-C',releaseDirectory,'.'],{cwd:root,stdout:'pipe',stderr:'pipe'});
const candidateArchiveError=await new Response(candidateArchiver.stderr).text();if(await candidateArchiver.exited!==0){await writeFile(join(directory,'showcase-candidate-archive-error.log'),candidateArchiveError,{mode:0o600});throw new Error('VERIFIED_CANDIDATE_ARCHIVE_FAILED');}
await chmod(candidateArchive,0o600);const candidateBytes=await readFile(candidateArchive);artifacts.push({name:'candidate.tar.gz',sha256:hash(candidateBytes),bytes:candidateBytes.length});
const after=await runtimeSourceFingerprint(root);if(after!==sourceFingerprint||await verificationSourceFingerprint(root,after)!==verificationFingerprint)throw new Error('SOURCE_CHANGED_DURING_SHOWCASE_PACKAGING');
await artifact('manifest.json',JSON.stringify({version:1,sourceFingerprint,verificationFingerprint,softwareVersion:candidate.softwareVersion,fileManifestSha256:candidate.fileManifestSha256,engineeringComplete:true,moduleCount:33,capabilityCount:187,cohorts:browser.cohorts,artifacts},null,2)+'\n');
console.log(JSON.stringify({showcaseDirectory:relative(root,destination),modules:33,capabilities:187,videoClips:clips.length,sourceFingerprint}));
