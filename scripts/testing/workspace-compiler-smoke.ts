import { developmentSmokeSelection } from './development-smoke';
import { runBrowserSuites } from './browser-runner';
import { workspaceBrowserBundler } from './development-host-profile';

// Functional compiler comparison uses the exact registered development cases.
// It does not change the public compiler choice or certify HMR performance.
const args=process.argv.slice(2),suiteId=args.find(arg=>arg.startsWith('--suite='))?.slice(8),raw=args.find(arg=>arg.startsWith('--bundler='))?.slice(10);
if(args.length!==2||!suiteId||!raw||args.some(arg=>!arg.startsWith('--suite=')&&!arg.startsWith('--bundler=')))throw new Error('EXPLICIT_COMPILER_SMOKE_SELECTION_REQUIRED');
const bundler=workspaceBrowserBundler(raw),plan=developmentSmokeSelection([suiteId]);
if(!plan.cases.length||plan.cases.some(test=>!test.workspace)||new Set(plan.cases.map(test=>JSON.stringify(test.workspace))).size!==1)throw new Error('EXACT_WORKSPACE_COMPILER_PROFILE_REQUIRED');
const result=await runBrowserSuites(plan.suites,{mode:'development-smoke',projects:['desktop'],caseOrder:'forward',caseSelection:plan.cases,workspace:{...plan.cases[0].workspace!,bundler}});
console.log(JSON.stringify({acceptance:false,scope:'workspace-compiler-functional-smoke',bundler,...result}));
