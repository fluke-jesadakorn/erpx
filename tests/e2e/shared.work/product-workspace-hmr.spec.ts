import {expect,test,type Page,type BrowserContext,type TestInfo} from '@playwright/test';
import {browserProjects} from '../../../scripts/testing/browser-results';
import {productWorkspaceCase,productWorkspaceDrawer,signInProductWorkspace,type PilotProduct} from '../../support/helpers/product-workspace';
import {measureWorkspaceHmr,workspaceHmrReady} from '../../support/helpers/workspace-hmr';

async function measure(page:Page,context:BrowserContext,info:TestInfo,app:PilotProduct){
 test.setTimeout(240_000);
 const input=await productWorkspaceCase(app),errors:string[]=[];
 page.on('pageerror',error=>errors.push(error.message));
 const ready=workspaceHmrReady(page,input.origin),authenticatedRouteAppearanceMs=await signInProductWorkspace(page,input);
 const{field}=await productWorkspaceDrawer(page,input),draft=`Synthetic unsaved ${app} workspace HMR draft`;
 await field.fill(draft);await ready();
 const initial=(await context.cookies()).find(cookie=>cookie.name==='__Host-erpx-product'&&cookie.domain===new URL(input.origin).hostname);expect(initial).toBeTruthy();
 const configuration={
  work:{source:'packages/feature-collaboration/src/components/social-home-shell.tsx',needle:'"Today"',label:'Today',locator:(label:string)=>page.getByRole('heading',{name:label,exact:true,includeHidden:true})},
  people:{source:'packages/feature-people_payroll/src/components/people-organization-detail.tsx',needle:'editPerson ? "Close editor" : "Edit profile"',label:'Close editor',replacement:(label:string)=>'editPerson ? '+JSON.stringify(label)+' : "Edit profile"',locator:(label:string)=>page.getByRole('button',{name:label,exact:true})},
  recruit:{source:'packages/feature-talent/src/components/talent-console.tsx',needle:'"Draft requisition"',label:'Draft requisition',locator:(label:string)=>page.getByRole('dialog',{name:label,exact:true})},
 }[app];
 await measureWorkspaceHmr({page,info,fixture:input.fixture,app,...configuration,authenticatedRouteAppearanceMs,errors,
  scope:`${app} desktop development HMR with a retained unsaved editor; sampled app process-tree RSS; excludes fixture preparation`,assertState:async(label)=>{
   const currentField=app==='recruit'?page.getByRole('dialog',{name:label,exact:true}).getByRole('textbox',{name:'Job title',exact:true}):field;
   if(app==='work')await expect(currentField).toHaveText(draft);else await expect(currentField).toHaveValue(draft);
   expect(new URL(page.url()).searchParams.get('workspaceSmoke')).toBe('preserved');
   expect((await context.cookies()).find(cookie=>cookie.name===initial!.name&&cookie.domain===initial!.domain)).toEqual(initial);
  }});
 // The browser context owns this unsaved draft; no business form is submitted.
}
const desktop=browserProjects(['desktop'],'Explicit twenty-edit development benchmark; acceptance browser matrix remains deferred.');
test('Work server edits preserve the update draft through twenty measured HMR updates',desktop,async({page,context},info)=>measure(page,context,info,'work'));
test('People client edits preserve the employee draft through twenty measured HMR updates',desktop,async({page,context},info)=>measure(page,context,info,'people'));
test('Recruit client edits preserve the requisition draft through twenty measured HMR updates',desktop,async({page,context},info)=>measure(page,context,info,'recruit'));
