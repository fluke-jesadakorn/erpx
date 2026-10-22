import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { loadCatalogue, repositoryRoot as root } from './catalogue';
import { loadRequiredCases } from './required-case-inventory';
import { runBrowserSuites } from './browser-runner';

type SmokeRegister={version:1;acceptance:false;cases:{suite:string;file:string;title:string;project:'desktop'}[];deferredSuites:{suite:string;reason:string}[]};
export function developmentSmokeSelection(suiteIds:readonly string[]){
 const register=JSON.parse(readFileSync(resolve(root,'tests/catalogue/development-smoke.json'),'utf8')) as SmokeRegister;
 if(register.version!==1||register.acceptance!==false)throw new Error('DEVELOPMENT_SMOKE_REGISTER_REQUIRED');
 const catalogue=loadCatalogue(),required=loadRequiredCases(root);
 for(const id of suiteIds)if(!register.cases.some(test=>test.suite===id)&&!register.deferredSuites?.some(item=>item.suite===id&&item.reason.trim()))throw new Error(`DEVELOPMENT_SMOKE_MAPPING_REQUIRED:${id}`);
 const selected=register.cases.filter(test=>suiteIds.includes(test.suite));
 for(const test of selected){const suite=catalogue.suites.find(suite=>suite.id===test.suite);
  if(test.project!=='desktop'||!suite?.files.includes(test.file)||!required.some(item=>item.runner==='playwright'&&item.project==='desktop'&&item.file===test.file&&item.title===test.title))throw new Error('DEVELOPMENT_SMOKE_IDENTITY_UNREGISTERED');}
 return {cases:selected,suites:catalogue.suites.filter(suite=>selected.some(test=>test.suite===suite.id)),deferred:suiteIds.filter(id=>!selected.some(test=>test.suite===id))};
}
if(import.meta.main){
 const args=process.argv.slice(2),suiteIds=args.filter(arg=>arg.startsWith('--suite=')).map(arg=>arg.slice(8));
 if(!suiteIds.length||args.some(arg=>!arg.startsWith('--suite=')&&!['--list','--json'].includes(arg)))throw new Error('EXPLICIT_DEVELOPMENT_SMOKE_SELECTION_REQUIRED');
 const plan=developmentSmokeSelection(suiteIds);
 if(args.some(arg=>['--list','--json'].includes(arg)))console.log(JSON.stringify({mode:'development-smoke',acceptance:false,...plan},null,2));
 else if(plan.suites.length)await runBrowserSuites(plan.suites,{mode:'development-smoke',projects:['desktop'],caseOrder:'forward',caseSelection:plan.cases});
 else console.log(JSON.stringify({mode:'development-smoke',acceptance:false,cases:[],deferred:plan.deferred}));
}
