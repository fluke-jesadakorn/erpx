import { expect, test, type Page } from '@playwright/test';
import { browserProjects } from '../../../scripts/testing/browser-results';
import { measureWorkspaceHmr, workspaceHmrCase, workspaceHmrReady } from '../../support/helpers/workspace-hmr';

async function signIn(page: Page, fixture: Awaited<ReturnType<typeof workspaceHmrCase>>) {
  const user=fixture.fixture.companies.desktop.users.learner;
  expect(new URL(page.url()).origin).toBe('https://account-dev-erpx.jesadakorn.com');
  await page.getByRole('textbox',{name:'Work email',exact:true}).fill(user.email);
  await page.locator('input[name="password"]').fill(fixture.fixture.password);
  await page.getByRole('button',{name:'Sign in',exact:true}).click();
}

test('Account client edits preserve the profile draft through twenty measured HMR updates', browserProjects(['desktop'],'Controlled desktop development HMR benchmark; viewport acceptance remains deferred.'), async ({page},info)=>{
  test.setTimeout(240_000);
  const fixture=await workspaceHmrCase(['account','apps']),errors:string[]=[];
  page.on('pageerror',error=>errors.push(error.message));
  const hmrReady=workspaceHmrReady(page,'https://account-dev-erpx.jesadakorn.com');
  await page.goto('https://account-dev-erpx.jesadakorn.com/my-profile?workspaceHmr=preserved',{timeout:120_000});
  const started=performance.now();await signIn(page,fixture);
  const field=(label:string)=>page.getByRole('textbox',{name:label,exact:true});
  await expect(field('Display name')).toBeEditable({timeout:90_000});
  const authenticatedRouteAppearanceMs=performance.now()-started,draft='Synthetic unsaved Account HMR draft';
  await field('Display name').fill(draft);await hmrReady();
  await measureWorkspaceHmr({page,info,fixture,app:'account',source:'packages/feature-identity_access/src/components/account-profile-fields.tsx',needle:'"Display name"',label:'Display name',locator:field,authenticatedRouteAppearanceMs,errors,scope:'Account desktop client HMR; sampled process-tree RSS; excludes fixture preparation',assertState:async label=>{
    await expect(field(label)).toHaveValue(draft);
    expect(new URL(page.url()).searchParams.get('workspaceHmr')).toBe('preserved');
  }});
  // Only an unsaved browser draft was edited. No profile save is submitted.
});

test('Apps server component edits preserve the session through twenty measured HMR updates', browserProjects(['desktop'],'Controlled desktop development HMR benchmark; viewport acceptance remains deferred.'), async ({page,context},info)=>{
  test.setTimeout(240_000);
  const fixture=await workspaceHmrCase(['account','apps']),errors:string[]=[];
  page.on('pageerror',error=>errors.push(error.message));
  const origin='https://apps-dev-erpx.jesadakorn.com';
  const hmrReady=workspaceHmrReady(page,origin);
  await page.goto(origin+'/?workspaceHmr=preserved',{timeout:120_000});
  const started=performance.now();await signIn(page,fixture);
  const heading=(label:string)=>page.getByRole('heading',{name:label,exact:true});
  await expect(heading('Choose a workspace')).toBeVisible({timeout:90_000});
  const authenticatedRouteAppearanceMs=performance.now()-started;
  await hmrReady();
  const initial=(await context.cookies()).find(cookie=>cookie.name==='__Host-erpx-product'&&cookie.domain==='apps-dev-erpx.jesadakorn.com');
  expect(initial).toBeTruthy();
  await measureWorkspaceHmr({page,info,fixture,app:'apps',source:'packages/workplace/src/entries/workspace.tsx',needle:'"Choose a workspace"',label:'Choose a workspace',locator:heading,authenticatedRouteAppearanceMs,errors,scope:'Apps desktop server component HMR; sampled process-tree RSS; excludes fixture preparation',assertState:async()=>{
    expect(new URL(page.url()).origin).toBe(origin);
    expect(new URL(page.url()).searchParams.get('workspaceHmr')).toBe('preserved');
    await expect(page.getByRole('link',{name:'Account',exact:true})).toBeVisible();
    const current=(await context.cookies()).find(cookie=>cookie.name===initial!.name&&cookie.domain===initial!.domain);
    expect(current).toEqual(initial);
  }});
});

test('Admin server component edits preserve the session through twenty measured HMR updates', browserProjects(['desktop'],'Controlled desktop development HMR benchmark; viewport acceptance remains deferred.'), async ({page,context},info)=>{
  test.setTimeout(240_000);
  const fixture=await workspaceHmrCase(['account','apps','admin']),errors:string[]=[];
  page.on('pageerror',error=>errors.push(error.message));
  const origin='https://admin-dev-erpx.jesadakorn.com';
  const hmrReady=workspaceHmrReady(page,origin);
  await page.goto(origin+'/?workspaceHmr=preserved',{timeout:120_000});
  const started=performance.now();await signIn(page,fixture);
  const heading=(label:string)=>page.getByRole('heading',{name:label,exact:true});
  await expect(heading('Choose a workspace')).toBeVisible({timeout:90_000});
  const authenticatedRouteAppearanceMs=performance.now()-started;
  await hmrReady();
  const initial=(await context.cookies()).find(cookie=>cookie.name==='__Host-erpx-product'&&cookie.domain==='admin-dev-erpx.jesadakorn.com');
  expect(initial).toBeTruthy();
  await measureWorkspaceHmr({page,info,fixture,app:'admin',source:'packages/workplace/src/entries/workspace.tsx',needle:'"Choose a workspace"',label:'Choose a workspace',locator:heading,authenticatedRouteAppearanceMs,errors,scope:'Admin desktop server component HMR; sampled process-tree RSS; excludes fixture preparation',assertState:async()=>{
    expect(new URL(page.url()).origin).toBe(origin);
    expect(new URL(page.url()).searchParams.get('workspaceHmr')).toBe('preserved');
    await expect(page.getByRole('link',{name:'Account',exact:true})).toBeVisible();
    const current=(await context.cookies()).find(cookie=>cookie.name===initial!.name&&cookie.domain===initial!.domain);
    expect(current).toEqual(initial);
  }});
});
