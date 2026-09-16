import {test,expect, type APIRequestContext} from '@playwright/test';
const credentials={email:process.env.ERPX_E2E_ADMIN_EMAIL??'jesadakorn.kirtnu@gmail.com',password:process.env.ERPX_E2E_ADMIN_PASSWORD??'12345678'};
async function choose(request:APIRequestContext,holding:string,role:string){
 const endpoint=`/api/v1/holdings/${holding}/assignments/active`;
 const state=(await(await request.get(endpoint)).json()).data;
 const choice=state.options.find((a:{role:string})=>a.role===role);
 expect(choice).toBeTruthy();
 const response=await request.post(endpoint,{headers:{'x-erpx-context-company':state.companyId},data:{assignmentId:choice.id,revision:state.revision}});
 expect(response.ok()).toBe(true);return choice.id;
}
test('new logins restore database holding, company and role with no browser cookies',async({context,page,browser,baseURL})=>{
 expect((await context.request.post('/api/auth/sign-in/email',{data:credentials})).ok()).toBe(true);
 expect((await context.request.post('/api/v1/holdings/acme/companies/active',{data:{companyId:'beacon-co'}})).ok()).toBe(true);
 const assignmentId=await choose(context.request,'acme','finance_approver');
 await page.goto('/holdings/acme/home');
 await expect(page.locator('[data-work-assignment]')).toHaveAttribute('data-work-assignment',assignmentId);
 const fresh=await browser.newContext({baseURL});
 try {
  expect(await fresh.cookies()).toHaveLength(0);
  const login=await fresh.newPage();await login.goto('/login');
  await login.getByLabel('Work email').fill(credentials.email);await login.getByLabel('Password',{exact:true}).fill(credentials.password);
  await login.getByRole('button',{name:'Sign in',exact:true}).click();
  await expect(login).toHaveURL(/\/holdings\/acme\/home$/);
  await expect(login.locator('[data-work-company]')).toHaveAttribute('data-work-company','beacon-co');
  await expect(login.locator('[data-work-assignment]')).toHaveAttribute('data-work-assignment',assignmentId);
  await choose(fresh.request,'acme','employee');
  const original=(await(await context.request.get('/api/v1/holdings/acme/assignments/active')).json()).data;
  expect(original.assignmentId).toBe(assignmentId);
  const northstar=await choose(fresh.request,'northstar','holding_owner');
  const next=await browser.newContext({baseURL});
  try {
   expect((await next.request.post('/api/auth/sign-in/email',{data:credentials})).ok()).toBe(true);
   const nextPage=await next.newPage();await nextPage.goto('/');
   await expect(nextPage).toHaveURL(/\/holdings\/northstar\/home$/);
   await expect(nextPage.locator('[data-work-assignment]')).toHaveAttribute('data-work-assignment',northstar);
   // Explicit login destinations still win over the saved business area.
   await next.request.post('/api/auth/sign-out');
   await nextPage.goto('/login?next=%2Fplatform%2Fhome');
   await nextPage.getByLabel('Work email').fill(credentials.email);await nextPage.getByLabel('Password',{exact:true}).fill(credentials.password);
   await nextPage.getByRole('button',{name:'Sign in',exact:true}).click();
   await expect(nextPage).toHaveURL(/\/platform\/home$/);
  } finally {await next.close();}
 } finally {
  await fresh.close();
  await context.request.post('/api/v1/holdings/acme/companies/active',{data:{companyId:'acme-co'}});
  await choose(context.request,'acme','holding_owner');
 }
});
