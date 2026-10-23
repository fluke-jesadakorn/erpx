import { developmentSmokeSelection } from './development-smoke';
import { runBrowserSuites } from './browser-runner';
import { workspaceBrowserBundler } from './development-host-profile';

// Functional compiler comparison uses the exact registered development cases.
// It does not change the public compiler choice or certify HMR performance.
const args=process.argv.slice(2),suiteId=args.find(arg=>arg.startsWith('--suite='))?.slice(8),raw=args.find(arg=>arg.startsWith('--bundler='))?.slice(10);
if(args.length!==2||!suiteId||!raw||args.some(arg=>!arg.startsWith('--suite=')&&!arg.startsWith('--bundler=')))throw new Error('EXPLICIT_COMPILER_SMOKE_SELECTION_REQUIRED');
const suiteIds=suiteId.split(',');if(new Set(suiteIds).size!==suiteIds.length)throw new Error('UNIQUE_COMPILER_SUITES_REQUIRED');
const bundler=workspaceBrowserBundler(raw),plan=developmentSmokeSelection(suiteIds);
if(!plan.cases.length||plan.cases.some(test=>!test.workspace))throw new Error('EXACT_WORKSPACE_COMPILER_PROFILE_REQUIRED');
const profiles=plan.cases.map(test=>test.workspace!);
if(profiles.some(profile=>Boolean(profile.consoleRetired)!==Boolean(profiles[0].consoleRetired)))throw new Error('MATCHED_COMPILER_CONSOLE_PROFILE_REQUIRED');
const apps=[...new Set(profiles.flatMap(profile=>profile.apps))];
const result=await runBrowserSuites(plan.suites,{mode:'development-smoke',projects:['desktop'],caseOrder:'forward',caseSelection:plan.cases,workspace:{...profiles[0],apps,bundler}});
console.log(JSON.stringify({acceptance:false,scope:'workspace-compiler-functional-smoke',bundler,...result}));
