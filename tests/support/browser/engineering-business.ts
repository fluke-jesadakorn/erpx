import {expect,type Browser,type BrowserContext,type Page,type TestInfo,type Locator} from '@playwright/test';
import {lstat,readFile,realpath} from 'node:fs/promises';
import {resolve} from 'node:path';
import {Pool} from 'pg';
import {createEngineeringBrowserContext} from "./engineering-browser-evidence";
import {decryptSensitiveSandboxEmailBody} from '../sandbox-email';

export const businessBase='/holdings/simulation',businessApi=`/api/v1${businessBase}`;
type User={email:string;role:string;roles?:string[];label:string;userId:string};
export type BusinessFixture={
 project:{id:string;rootId:string;discoveryId:string;buildId:string;taskId:string;costId:string;costJournalId:string;costMinor:number;recipientId:string};
 preventive:{assetId:string;id:string;firstDueAt:number;warehouseId:string;spareId:string;openingCountId:string;initialValueMinor:number};
 production:{warehouseId:string;materialId:string;outputId:string;specificationId:string;resourceId:string;openingCountId:string;initialQuantityMilli:number;initialValueMinor:number;salesDemandId:string;incomingSupplyId:string};
 ecommerce:{channelId:string;listingId:string;cartId:string;warehouseId:string;productId:string;partyId:string;openingCountId:string;initialQuantityMilli:number;initialValueMinor:number;amountMinor:number};
 fieldService:{id:string;ticketId:string;agreementId:string;amountMinor:number;bankAccountId:string;bankTransactionId:string};
 retail:{registerId:string;registerName:string;warehouseId:string;productId:string;productName:string;openingCountId:string;initialQuantityMilli:number;initialValueMinor:number;priceMinor:number};
 customerAuthority:{bindingId:string;orderId:string;excludedOrderId:string;recipientUserId:string;recipientEmail:string;invitedBindingId:string;invitedEmail:string;unrelatedUserId:string};
};
type State={synthetic:true;companyId:string;branchId:string;chartId:string;periodId:string;today:string;accounting:{inventory:string;wip:string;labor:string;overhead:string};personas:{sourceUserId:string;reviewerUserId:string;financeUserId:string;inspectorUserId:string};viewports:Record<string,BusinessFixture>};
export async function businessFixture(info:TestInfo){
 const run=process.env.ERPX_DEMO_RUN;if(!run||!/^[a-f0-9]{12}$/.test(run))throw new Error('EXACT_PREPARED_BROWSER_RUN_REQUIRED');
 const directory=resolve('.runtime/browser',run),credentials=JSON.parse(await readFile(resolve(directory,'credentials.json'),'utf8')) as {password:string;users:User[]},snapshot=JSON.parse(await readFile(resolve(directory,'state.json'),'utf8')) as {fixture:{engineeringBusiness:State}},state=snapshot.fixture.engineeringBusiness;
 expect(state.synthetic).toBe(true);const fixture=state.viewports[info.project.name];expect(fixture,'different native sources for desktop and mobile').toBeTruthy();
 const manifest=JSON.parse(await readFile(resolve(directory,'databases.json'),'utf8')) as {databases:{H001_DB:string}},url=new URL(manifest.databases.H001_DB);
 expect([url.hostname,url.port,url.username,url.pathname]).toEqual(['127.0.0.1','5432','erpx_test_admin',`/erpx_browser_${run}_h001_db`]);
 return {run,state,fixture,credentials,db:new Pool({connectionString:url.href,max:1})};
}
type Loaded=Awaited<ReturnType<typeof businessFixture>>;
/** Reads one isolated encrypted sandbox message and returns its private invitation URL in memory only. */
type CustomerPortalMailLinkOptions={subject:string;purpose:'invitation'|'verification';deliveryKey?:string;createdAfter?:number};
async function customerPortalControlUrl(loaded:Loaded){
 const directory=resolve('.runtime/browser',loaded.run),manifest=JSON.parse(await readFile(resolve(directory,'databases.json'),'utf8')) as {databases:{CONTROL_DB?:string}},database=manifest.databases.CONTROL_DB;
 if(typeof database!=='string')throw new Error('CUSTOMER_PORTAL_CONTROL_DATABASE_REQUIRED');
 const url=new URL(database);
 if(url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!==`/erpx_browser_${loaded.run}_control_db`)throw new Error('CUSTOMER_PORTAL_CONTROL_DATABASE_SCOPE_INVALID');
 return {directory,url};
}
export async function customerPortalAccountState(loaded:Loaded,email:string){
 const {url}=await customerPortalControlUrl(loaded),control=new Pool({connectionString:url.href,max:1});
 try{
  const row=(await control.query<{id:string;emailVerified:boolean|number|string}>(`SELECT id,"emailVerified" FROM "user" WHERE lower(email)=lower($1)`,[email])).rows[0];
  if(!row)return null;
  if(![true,false,0,1,'0','1'].includes(row.emailVerified))throw new Error('CUSTOMER_PORTAL_ACCOUNT_VERIFICATION_STATE_INVALID');
  return {id:row.id,emailVerified:row.emailVerified===true||row.emailVerified===1||row.emailVerified==='1'};
 }finally{await control.end();}
}
/** Reads one isolated encrypted sandbox message and returns its private link in memory only. */
export async function customerPortalEmailLink(loaded:Loaded,email:string,options:CustomerPortalMailLinkOptions){
 const {directory,url}=await customerPortalControlUrl(loaded);
 const runtimePath=resolve(directory,'runtime.json'),runtimeStat=await lstat(runtimePath);
 if(!runtimeStat.isFile()||(runtimeStat.mode&0o077)!==0||await realpath(runtimePath)!==runtimePath)throw new Error('CUSTOMER_PORTAL_PRIVATE_RUNTIME_REQUIRED');
 const runtime=JSON.parse(await readFile(resolve(directory,'runtime.json'),'utf8')) as {secret?:unknown};
 if(typeof runtime.secret!=='string'||!runtime.secret)throw new Error('CUSTOMER_PORTAL_ENCRYPTION_SECRET_REQUIRED');
 const control=new Pool({connectionString:url.href,max:1});
 try{
  let delivery:{delivery_key:string;recipient:string;subject:string;body_text:string;body_ciphertext:string|null;provider_id:string}|undefined;
  const query=options.deliveryKey
   ?{sql:'SELECT delivery_key,recipient,subject,body_text,body_ciphertext,provider_id FROM sandbox_email_deliveries WHERE delivery_key=$1',values:[options.deliveryKey]}
   :{sql:'SELECT delivery_key,recipient,subject,body_text,body_ciphertext,provider_id FROM sandbox_email_deliveries WHERE recipient=$1 AND subject=$2 AND created_at>=$3 ORDER BY created_at DESC LIMIT 1',values:[email,options.subject,options.createdAfter??0]};
  const invitationId=options.deliveryKey?.startsWith('customer-portal-invitation-')?options.deliveryKey.slice('customer-portal-invitation-'.length):undefined;
  let invitationState:{state:string;last_error:string|null;attempts:number}|undefined;
  // The first retry backoff is 60 seconds. Keep polling the invitation and
  // sandbox receipt through that window instead of failing after 30 seconds.
  for(let attempt=0;attempt<180&&!delivery;attempt++){
   delivery=(await control.query<{delivery_key:string;recipient:string;subject:string;body_text:string;body_ciphertext:string|null;provider_id:string}>(query.sql,query.values)).rows[0];
   if(delivery)break;
   if(invitationId){
    invitationState=(await loaded.db.query<{state:string;last_error:string|null;attempts:number}>('SELECT state,last_error,attempts FROM customer_portal_invitations WHERE id=$1',[invitationId])).rows[0];
    if(!invitationState||['accepted','expired','failed','revoked','superseded'].includes(invitationState.state))throw new Error(`CUSTOMER_PORTAL_SENSITIVE_DELIVERY_NOT_REDACTED:${JSON.stringify(invitationState??null)}`);
   }
   if(!delivery)await new Promise(resolve=>setTimeout(resolve,500));
  }
  if(!delivery){
   if(invitationId)invitationState=(await loaded.db.query<{state:string;last_error:string|null;attempts:number}>('SELECT state,last_error,attempts FROM customer_portal_invitations WHERE id=$1',[invitationId])).rows[0];
   throw new Error(`CUSTOMER_PORTAL_SENSITIVE_DELIVERY_NOT_REDACTED:${JSON.stringify(invitationState??null)}`);
  }
  if(delivery.recipient!==email||delivery.subject!==options.subject||delivery.body_text!=='[redacted sensitive email body]'||!delivery.body_ciphertext)throw new Error('CUSTOMER_PORTAL_SENSITIVE_DELIVERY_NOT_REDACTED');
  let body:string;
  try{body=decryptSensitiveSandboxEmailBody(runtime.secret,delivery);}catch{throw new Error('CUSTOMER_PORTAL_SENSITIVE_DELIVERY_INVALID');}
  const links=body.match(/https?:\/\/[^\s<>\"]+/g)??[];
  if(links.length!==1)throw new Error('CUSTOMER_PORTAL_INVITATION_LINK_INVALID');
  const invitation=new URL(links[0]),origin=new URL(process.env.ERPX_E2E_BASE_URL??'http://127.0.0.1:3104');
  const valid=options.purpose==='invitation'
   ?invitation.pathname==='/customer-portal/enroll'&&invitation.search===''&&!!new URLSearchParams(invitation.hash.slice(1)).get('token')
   :invitation.pathname==='/api/auth/verify-email'&&!!invitation.searchParams.get('token')&&invitation.searchParams.get('callbackURL')===`${origin.origin}/customer-portal/enroll?verified=1`;
  if(invitation.origin!==origin.origin||!valid)throw new Error('CUSTOMER_PORTAL_INVITATION_LINK_SCOPE_INVALID');
  if(options.purpose==='invitation'){
   const invitationId=options.deliveryKey?.startsWith('customer-portal-invitation-')?options.deliveryKey.slice('customer-portal-invitation-'.length):undefined;
   if(!invitationId)throw new Error('CUSTOMER_PORTAL_INVITATION_ID_REQUIRED');
   let state:{state:string;last_error:string|null}|undefined;
   // A failed sandbox attempt remains queued until its first exponential backoff expires.
   // Wait through that retry window so a transient worker/database error does not
   // turn an eventually delivered invitation into a false-negative browser run.
   for(let attempt=0;attempt<300;attempt++){
    state=(await loaded.db.query<{state:string;last_error:string|null}>('SELECT state,last_error FROM customer_portal_invitations WHERE id=$1',[invitationId])).rows[0];
    if(state?.state==='sent')break;
    if(!state||['accepted','expired','failed','revoked','superseded'].includes(state.state))throw new Error(`CUSTOMER_PORTAL_INVITATION_DELIVERY_NOT_SENT:${state?.state??'missing'}:${state?.last_error??'none'}`);
    await new Promise(resolve=>setTimeout(resolve,250));
   }
   if(state?.state!=='sent')throw new Error(`CUSTOMER_PORTAL_INVITATION_DELIVERY_NOT_SENT:${state?.state??'missing'}:${state?.last_error??'none'}`);
  }
  return invitation.href;
 }finally{await control.end();}
}
/** Navigate without recording the private email link as a Playwright action argument. */
export async function navigateCustomerPortalMailLink(page:Page,link:string){
 try{
  await Promise.all([
   page.waitForNavigation({waitUntil:'commit',timeout:45_000}),
   page.evaluate(url=>window.location.assign(url),link),
  ]);
 }catch{throw new Error('CUSTOMER_PORTAL_MAIL_LINK_NAVIGATION_FAILED');}
}
export async function businessActor(browser:Browser,info:TestInfo,loaded:Loaded,persona:'source'|'reviewer'|'finance'|'inspector',path:string){
 const userId=loaded.state.personas[`${persona}UserId`],user=loaded.credentials.users.find(row=>row.userId===userId);expect(user,`normal ${persona} credentials`).toBeTruthy();
 const role=persona==='finance'?'finance_approver':persona==='inspector'?'quality_inspector':'holding_owner',context=await createEngineeringBrowserContext(browser,info),page=await context.newPage(),origin=process.env.ERPX_E2E_BASE_URL??'http://127.0.0.1:3104';
 const login=await context.request.post('/api/auth/sign-in/email',{headers:{Origin:origin},data:{email:user!.email,password:loaded.credentials.password}});expect(login.ok(),await login.text()).toBe(true);
 const response=await context.request.get(`${businessApi}/assignments/active`);expect(response.ok(),await response.text()).toBe(true);const active=(await response.json()).data as {revision:number;options:Array<{id:string;role:string;companyId:string}>};
 const option=active.options.find(row=>row.role===role&&row.companyId===loaded.state.companyId);expect(option,`live ${persona} Company role`).toBeTruthy();
 const selected=await context.request.post(`${businessApi}/assignments/active`,{headers:{Origin:origin},data:{assignmentId:option!.id,revision:active.revision}});expect(selected.ok(),await selected.text()).toBe(true);
 await businessOpen(page,path);return {context,page,userId};
}
/** Portal access uses the normal verified credential session, without Console role selection. */
export async function businessCustomerActor(browser:Browser,info:TestInfo,loaded:Loaded,userId:string){
 const user=loaded.credentials.users.find(row=>row.userId===userId);expect(user,'normal verified customer credentials').toBeTruthy();
 const context=await createEngineeringBrowserContext(browser,info),page=await context.newPage(),origin=process.env.ERPX_E2E_BASE_URL??'http://127.0.0.1:3104';
 const login=await context.request.post('/api/auth/sign-in/email',{headers:{Origin:origin},data:{email:user!.email,password:loaded.credentials.password}});expect(login.ok(),await login.text()).toBe(true);
 await page.goto('/customer-portal/simulation',{waitUntil:'domcontentloaded'});await expect(page.getByRole('heading',{name:'Customer documents',exact:true})).toBeVisible();return {context,page,userId,email:user!.email};
}
/** The daily retail operator signs in on POS Front's own host with the same real staff account. */
export async function businessPosFront(context:BrowserContext,page:Page,loaded:Loaded,userId:string){
 const user=loaded.credentials.users.find(row=>row.userId===userId);expect(user,'normal retail staff credentials').toBeTruthy();
 const company=(await loaded.db.query("SELECT slug FROM organization_nodes WHERE id=$1 AND company_id=$1 AND kind='company' AND status='active'",[loaded.state.companyId])).rows[0] as {slug:string}|undefined;expect(company).toBeTruthy();
 const root=new URL(process.env.ERPX_E2E_BASE_URL??'http://127.0.0.1:3104'),origin=`http://dev-pos.localhost:${root.port}`,path=`/simulation/${encodeURIComponent(company!.slug)}/holding`,url=`${origin}${path}`;
 await page.goto(`${origin}/login?next=${encodeURIComponent(path)}`,{waitUntil:'domcontentloaded'});
 await page.locator('input[type="email"]').fill(user!.email);await page.locator('input[type="password"]').fill(loaded.credentials.password);
 await Promise.all([page.waitForURL(destination=>destination.origin===origin&&destination.pathname===path),page.getByRole('button',{name:/^(Sign in|เข้าสู่ระบบ)$/i}).click()]);
 const endpoint=`${origin}/api/v1/front/simulation/${encodeURIComponent(company!.slug)}/pos/roles`,headers={Origin:origin,'x-erpx-front-location':'holding'};
 const consoleRoles=await context.request.get(`${businessApi}/assignments/active`);expect(consoleRoles.ok()).toBe(true);const eligible=(await consoleRoles.json()).data as {options:Array<{id:string;role:string;companyId:string}>};
 const option=eligible.options.find(row=>row.role==='holding_owner'&&row.companyId===loaded.state.companyId);expect(option,'current scoped retail staff role').toBeTruthy();
 const roles=await context.request.get(endpoint,{headers});expect(roles.ok()).toBe(true);const active=(await roles.json()).data as {valid:boolean;revision:number;assignmentId:string|null;companyId:string};expect(active.companyId).toBe(loaded.state.companyId);expect(active.valid).toBe(true);
 if(active.assignmentId!==option!.id){const selected=await context.request.post(endpoint,{headers,data:{location:'holding',assignmentId:option!.id,revision:active.revision}});expect(selected.ok()).toBe(true);}
 await page.goto(url,{waitUntil:'domcontentloaded'});await expect(page.locator('[data-work-assignment]')).toHaveAttribute('data-work-assignment',option!.id);await expect(page.locator('[data-work-stale]')).toHaveAttribute('data-work-stale','false');
 if(await page.locator('html').getAttribute('lang')!=='en'){await page.getByRole('button',{name:'EN',exact:true}).click();await expect(page.locator('html')).toHaveAttribute('lang','en');}
 return {url,api:`/api/v1/front/simulation/${encodeURIComponent(company!.slug)}/pos`};
}
export async function businessOpen(page:Page,path:string){await page.goto(path.startsWith('/')?path:`${businessBase}/${path}`,{waitUntil:'domcontentloaded'});await expect(page.locator('[data-work-stale]')).toHaveAttribute('data-work-stale','false');}
export async function businessReady(page:Page){await expect(page.locator('button[data-client-managed-ready=true]').first()).toBeAttached();}
export function businessForm(page:Page,title:string):Locator{return page.locator('form').filter({has:page.getByRole('heading',{name:title,exact:true})});}
export async function businessClick(page:Page,path:string,click:()=>Promise<void>){
 const pending=page.waitForResponse(response=>response.request().method()==='POST'&&new URL(response.url()).pathname===`${businessApi}/${path}`);await click();const response=await pending,body=await response.json();expect(response.ok(),JSON.stringify(body)).toBe(true);return body.data as Record<string,unknown>;
}
export async function businessRead<T>(page:Page,path:string):Promise<T>{const response=await page.request.get(`${businessApi}/${path}`);expect(response.ok(),await response.text()).toBe(true);return (await response.json()).data as T;}
/** The normal request carries the role/version that the visible page rendered. */
export async function businessRequest(page:Page,path:string,data:unknown,key=crypto.randomUUID()){
 const stamp=await page.locator('[data-work-revision]').evaluate(node=>({revision:(node as HTMLElement).dataset.workRevision!,company:(node as HTMLElement).dataset.workCompany!,assignment:(node as HTMLElement).dataset.workAssignment!}));
 return page.request.post(`${businessApi}/${path}`,{headers:{Origin:new URL(page.url()).origin,'Idempotency-Key':key,'x-erpx-context-revision':stamp.revision,'x-erpx-context-company':stamp.company,'x-erpx-context-assignment':stamp.assignment},data});
}
export async function businessPost(page:Page,path:string,data:unknown,key?:string){const response=await businessRequest(page,path,data,key);const body=await response.json();expect(response.ok(),JSON.stringify(body)).toBe(true);return body.data as Record<string,unknown>;}
export async function businessEvidence(page:Page,info:TestInfo,label:string){await info.attach(label,{body:await page.screenshot({fullPage:true}),contentType:'image/png'});}
export async function closeBusiness(contexts:BrowserContext[],db:Pool){for(const context of contexts)await context.close();await db.end();}
