import { readFile, readdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { runBrowserSuites } from '../testing/browser-runner';
import { loadCatalogue } from '../testing/catalogue';
import { workspaceBrowserBundler } from '../testing/development-host-profile';

const args=process.argv.slice(2),raw=args[0]?.startsWith('--bundler=')?args[0].slice(10):undefined;
if(args.length!==1||!raw)throw new Error('EXPLICIT_HMR_BUNDLER_REQUIRED');
const bundler=workspaceBrowserBundler(raw),suite=loadCatalogue().suites.find(suite=>suite.id==='learning.workspace-hmr.e2e');
if(!suite)throw new Error('REGISTERED_LEARN_HMR_SUITE_REQUIRED');
const result=await runBrowserSuites([suite],{mode:'development-smoke',projects:['desktop'],caseOrder:'forward',workspace:{apps:['account','apps','learn'],entryApp:'learn',bundler},caseSelection:[{suite:suite.id,file:'tests/e2e/learning/workspace-hmr.spec.ts',project:'desktop',title:'Learn client edits preserve the draft through twenty measured HMR updates'}]});
const files=(await readdir(result.evidenceDirectory,{recursive:true,withFileTypes:true})).filter(file=>file.isFile()&&file.name==='workspace-performance.json');
if(files.length!==1)throw new Error('EXACT_HMR_MEASUREMENT_REQUIRED');
const sample=JSON.parse(await readFile(join(files[0].parentPath,files[0].name),'utf8'));
if(sample.acceptance!==false||sample.bundler!==bundler||sample.sourceFingerprint!==result.runtimeFingerprint||sample.samples.length!==20||sample.reloads||sample.errors.length)throw new Error('HMR_MEASUREMENT_BINDING_FAILED');
const receipt={acceptance:false,scope:'Learn desktop HMR; no monolith/other-app baseline or production acceptance',...result,measurement:sample};
const path=join(result.evidenceDirectory,'performance.json');await writeFile(path,JSON.stringify(receipt,null,2)+'\n',{mode:0o600});
console.log(JSON.stringify({receipt:path,acceptance:false,bundler,p95Ms:sample.p95Ms,sampledPeakAppRssBytes:sample.peakAppRssBytes,target:sample.target}));
