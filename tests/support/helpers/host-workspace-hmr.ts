import {expect,type Page,type Locator,type TestInfo} from '@playwright/test';
import {measureWorkspaceHmr,workspaceHmrCase} from './workspace-hmr';

export async function measureFrontWorkspaceHmr(page:Page,info:TestInfo,app:'pos'|'pms',apps:string[],ready:()=>Promise<void>,errors:string[],started:number){
 await ready();
 const dialog=page.getByRole('dialog',{name:app==='pos'?/^Customer · /:'Write a post',exact:app==='pms'});
 const field=dialog.locator('textarea,input:not([type="hidden"]):not([type="file"]),[contenteditable="true"]').first();
 await expect(field).toBeEditable();
 const rich=await field.getAttribute('contenteditable')==='true',original=rich?await field.innerText():await field.inputValue(),draft='Retained fictional HMR draft';
 await field.fill(draft);
 const spec=app==='pos'?{
  source:'packages/feature-pos/src/components/standard-pos-sale-workspace.tsx',needle:'{t("ลูกค้า", "Customer")} ·{" "}',label:'Customer',
  replacement:(label:string)=>'{t("ลูกค้า", '+JSON.stringify(label)+')} ·{" "}',
  locator:(label:string)=>page.getByRole('dialog',{name:new RegExp('^'+label.replace(/[.*+?^${}()|[\]\\]/g,'\\$&')+' · ')}),
 }:{
  source:'packages/feature-pms/src/components/standard-pms-today.tsx',needle:'title={t("เขียนโพสต์", "Write a post")}',label:'Write a post',
  replacement:(label:string)=>'title={t("เขียนโพสต์", '+JSON.stringify(label)+')}',
  locator:(label:string)=>page.getByRole('dialog',{name:label,exact:true}),
 };
 const selector='textarea,input:not([type="hidden"]):not([type="file"]),[contenteditable="true"]';
 await measureWorkspaceHmr({page,info,app,fixture:await workspaceHmrCase(apps),...spec,
  assertState:async label=>{const current=spec.locator(label).locator(selector).first();if(rich)await expect(current).toHaveText(draft);else await expect(current).toHaveValue(draft);},
  authenticatedRouteAppearanceMs:performance.now()-started,errors,scope:app+' Front normal Account SSO and retained editor HMR'});
 await field.fill(original);
}

export async function measureSharedWorkspaceHmr(page:Page,info:TestInfo,app:'portal'|'legal-portal'|'forms'|'booking',apps:string[],ready:()=>Promise<void>,errors:string[],started:number){
 await ready();let field:Locator|undefined,original='',label:string,source:string,needle:string,replacement:(label:string)=>string,locator:(label:string)=>Locator;
 if(app==='portal'){
  field=page.getByLabel('Your name',{exact:true});label='Accept your Customer Portal invitation';source='packages/feature-customer_authority/src/components/customer-portal-enrollment.tsx';needle='<h1>'+label+'</h1>';replacement=value=>'<h1>{'+JSON.stringify(value)+'}</h1>';locator=value=>page.getByRole('heading',{name:value,exact:true});
 }else if(app==='forms'){
  field=page.getByLabel('Name',{exact:true});label='Send request';source='packages/feature-marketing/src/components/public-marketing-form.tsx';needle='"Submitting…" : "Send request"';replacement=value=>'"Submitting…" : '+JSON.stringify(value);locator=value=>page.getByRole('button',{name:value,exact:true});
 }else if(app==='booking'){
  field=page.getByLabel('Adults per room',{exact:true});label='Search availability';source='packages/feature-pms/src/components/public-booking-reservation.tsx';needle='>Search availability</Button>';replacement=value=>'>'+value+'</Button>';locator=value=>page.getByRole('button',{name:value,exact:true});
 }else{
  label='Accept your Legal invitation';source='packages/feature-legal/src/components/legal-enrollment.tsx';needle='<main className={s.workspace} style={{maxWidth:600,padding:32}}><h1>'+label+'</h1>';replacement=value=>'<main className={s.workspace} style={{maxWidth:600,padding:32}}><h1>{'+JSON.stringify(value)+'}</h1>';locator=value=>page.getByRole('heading',{name:value,exact:true});
 }
 const draft=app==='booking'?'3':'Retained fictional visitor draft',url=page.url();
 if(field){await expect(field).toBeEditable();original=await field.inputValue();await field.fill(draft);}
 const fixture=await workspaceHmrCase(apps);
 await measureWorkspaceHmr({page,info,app,fixture,source,needle,label,replacement,locator,collectFailedBaselineTiming:['booking','portal'].includes(app)&&fixture.runtime.bundler==='webpack',
  assertState:async()=>{expect(page.url()).toBe(url);if(field)await expect(field).toHaveValue(draft);if(app==='legal-portal'){const handoff=page.getByText('Continue on ERPX Account',{exact:true});await expect(handoff).toHaveAttribute('aria-disabled','true');await expect(handoff).not.toHaveAttribute('href');}},
  authenticatedRouteAppearanceMs:performance.now()-started,errors,scope:app+' public visitor HMR; '+(field?'retained form draft':'disabled central Account handoff')});
 if(field)await field.fill(original);
}
