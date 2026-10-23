import { readFile, readdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { runBrowserSuites } from '../testing/browser-runner';
import { loadCatalogue } from '../testing/catalogue';
import { workspaceBrowserBundler } from '../testing/development-host-profile';

const args=process.argv.slice(2),raw=args.find(arg=>arg.startsWith('--bundler='))?.slice(10),app=args.find(arg=>arg.startsWith('--app='))?.slice(6)??'learn';
if(!raw||args.filter(arg=>arg.startsWith('--bundler=')).length!==1||args.filter(arg=>arg.startsWith('--app=')).length>1||args.some(arg=>!arg.startsWith('--bundler=')&&!arg.startsWith('--app=')))throw new Error('EXPLICIT_HMR_BUNDLER_REQUIRED');
const benchmarks={
 learn:{suite:'learning.workspace-hmr.e2e',apps:['account','apps','learn'],file:'tests/e2e/learning/workspace-hmr.spec.ts',title:'Learn client edits preserve the draft through twenty measured HMR updates',scope:'Learn desktop client HMR'},
 account:{suite:'identity_access.workspace-hmr.e2e',apps:['account','apps'],file:'tests/e2e/identity_access/workspace-hmr.spec.ts',title:'Account client edits preserve the profile draft through twenty measured HMR updates',scope:'Account desktop client HMR'},
 apps:{suite:'identity_access.workspace-hmr.e2e',apps:['account','apps'],file:'tests/e2e/identity_access/workspace-hmr.spec.ts',title:'Apps server component edits preserve the session through twenty measured HMR updates',scope:'Apps desktop server component HMR'},
 admin:{suite:'identity_access.workspace-hmr.e2e',apps:['account','apps','admin'],file:'tests/e2e/identity_access/workspace-hmr.spec.ts',title:'Admin server component edits preserve the session through twenty measured HMR updates',scope:'Admin desktop server component HMR'},
 work:{suite:'shared.work.product-workspace-hmr.e2e',apps:['account','apps','work'],file:'tests/e2e/shared.work/product-workspace-hmr.spec.ts',title:'Work server edits preserve the update draft through twenty measured HMR updates',scope:'Work desktop server component HMR'},
 people:{suite:'shared.work.product-workspace-hmr.e2e',apps:['account','apps','people'],file:'tests/e2e/shared.work/product-workspace-hmr.spec.ts',title:'People client edits preserve the employee draft through twenty measured HMR updates',scope:'People desktop client HMR'},
 recruit:{suite:'shared.work.product-workspace-hmr.e2e',apps:['account','apps','recruit'],file:'tests/e2e/shared.work/product-workspace-hmr.spec.ts',title:'Recruit client edits preserve the requisition draft through twenty measured HMR updates',scope:'Recruit desktop client HMR'},
};
if(!Object.hasOwn(benchmarks,app))throw new Error('REGISTERED_HMR_APP_REQUIRED');
const benchmark=benchmarks[app as keyof typeof benchmarks],bundler=workspaceBrowserBundler(raw),suite=loadCatalogue().suites.find(suite=>suite.id===benchmark.suite);
if(!suite)throw new Error('REGISTERED_HMR_SUITE_REQUIRED');
const result=await runBrowserSuites([suite],{mode:'development-smoke',projects:['desktop'],caseOrder:'forward',workspace:{apps:benchmark.apps,entryApp:app,bundler},caseSelection:[{suite:suite.id,file:benchmark.file,project:'desktop',title:benchmark.title}]});
const files=(await readdir(result.evidenceDirectory,{recursive:true,withFileTypes:true})).filter(file=>file.isFile()&&file.name==='workspace-performance.json');
if(files.length!==1)throw new Error('EXACT_HMR_MEASUREMENT_REQUIRED');
const sample=JSON.parse(await readFile(join(files[0].parentPath,files[0].name),'utf8'));
if((app!=='learn'&&sample.app!==app)||sample.acceptance!==false||sample.bundler!==bundler||sample.sourceFingerprint!==result.runtimeFingerprint||sample.samples.length!==20||sample.reloads||sample.errors.length)throw new Error('HMR_MEASUREMENT_BINDING_FAILED');
const receipt={acceptance:false,scope:benchmark.scope+'; no monolith/other-app baseline or production acceptance',...result,measurement:sample};
const path=join(result.evidenceDirectory,'performance.json');await writeFile(path,JSON.stringify(receipt,null,2)+'\n',{mode:0o600});
console.log(JSON.stringify({receipt:path,acceptance:false,bundler,p95Ms:sample.p95Ms,sampledPeakAppRssBytes:sample.peakAppRssBytes,target:sample.target}));
