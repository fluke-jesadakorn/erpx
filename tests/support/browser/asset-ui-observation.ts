/** Actual Asset owner UI observations. No owner mocks or acceptance promotion. */
import {expect,type BrowserContext,type Locator,type Page,type TestInfo} from '@playwright/test';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {existsSync,lstatSync,readFileSync,realpathSync,writeFileSync} from 'node:fs';
import {dirname,relative,resolve} from 'node:path';
import {runtimeSourceFingerprint,verificationSourceFingerprint} from '../../../scripts/local/source-fingerprint';
import {readProductHostLease} from '../../../scripts/testing/product-host-cohort';

const root=resolve(process.cwd()),routeSource="packages/feature-assets/src/routes/holdings/[holdingSlug]/services/assets/register/page.tsx",targetSource='packages/feature-assets/src/components/asset-editor.tsx';
export const assetRegisterPath='/holdings/simulation/services/assets/register';
const endpoint='/api/v1/holdings/simulation/assets',roles='/api/v1/holdings/simulation/assignments/active';
const hash=(bytes:Uint8Array|string)=>createHash('sha256').update(bytes).digest('hex');
export type AssetUiWork={valid:boolean;companyId:string;userId:string;assignmentId:string;revision:number;options:{id:string;role:string}[]};
type AssetUiItem={id:string;version:number;code:string;name:string;category:string;currency:string;cost_minor:number};
type AssetUiData={items:AssetUiItem[];nodes:{id:string;kind:string}[];canCreate:boolean};
type Surface={component:string;line:number;props:Record<string,unknown>;conditions:string[]};
// This source review is specific to the exact canonical Asset LIST owner.
// A changed owner/standard cannot inherit the conditional finding silently.
const assetRegisterNonDocumentSources = [
 {
  "path": "docs/ux-ui/ui-standardization.md",
  "sha256": "ce6f94a0d76c763230ff90ce37b4b81e9a61baf0e9688e3cc7c3ddab30c42557",
  "bytes": 18934
 },
 {
  "path": "docs/contracts/next/finance.md",
  "sha256": "32ba46337410d782118e667c5505e6cc1e0813019c9a60a92422ac8ebea78841",
  "bytes": 112872
 },
 {
  "path": "scripts/ui/acceptance.mjs",
  "sha256": "e287fc9f64362e03d10a332504623c37394ffe8ca5c2b38b690bc3212a07f2a9",
  "bytes": 28133
 },
 {
  "path": "packages/feature-assets/src/routes/holdings/[holdingSlug]/services/assets/register/page.tsx",
  "sha256": "8e4f05c0ac99d45585690ef5b60f0b2f08f8efb96c31188d53b317f5d79cb8f9",
  "bytes": 427
 },
 {
  "path": "packages/workplace/src/routes/holdings/[holdingSlug]/services/assets/register/_screen.tsx",
  "sha256": "7ff26ee185dd44887d166e6a10369a16bf31633bb0f366e5c21a27896e537ea6",
  "bytes": 5620
 },
 {
  "path": "packages/feature-assets/src/components/asset-maintenance-console.tsx",
  "sha256": "88f734f9cc8daf5047f4793499cdb3079599eef66873ff905e879c496b1edd68",
  "bytes": 17004
 },
 {
  "path": "packages/feature-assets/src/components/asset-editor.tsx",
  "sha256": "9d8c6f068ddd08e0deb92041f8da16552ae367049a2271a3c331ad9c2e18748e",
  "bytes": 5606
 },
 {
  "path": "packages/feature-work_projection/src/lib/destination-registry.ts",
  "sha256": "ad9915913d55392a6d416f65591c1765e470efcfea7a9100d2876cb5768724b5",
  "bytes": 33536
 },
 {
  "path": "packages/feature-work_projection/src/lib/section-workspaces.ts",
  "sha256": "a266f26a4e2c6f64d6bf8f8ed7c6b8445cc56cb2e1ba74f3fb504fb11213aba9",
  "bytes": 4726
 },
 {
  "path": "packages/workplace/src/components/section-workspace.tsx",
  "sha256": "8503d384e8eeb72166c4ab120de2917f74f2ac1386fd78d3cb90677dcbd99bb2",
  "bytes": 2237
 },
 {
  "path": "packages/ui/src/components/table.tsx",
  "sha256": "6537f8dcd7c7601bafd062e47ffa50ed80f03c0632e8bd3ebad474070f4675ed",
  "bytes": 4546
 }
] as const;
function assertAssetRegisterNonDocumentSources(){
 for(const binding of assetRegisterNonDocumentSources){const path=resolve(root,binding.path);expect(realpathSync.native(path)).toBe(path);expect(lstatSync(path).isFile()).toBe(true);const bytes=readFileSync(path);expect(bytes.length).toBe(binding.bytes);expect(hash(bytes)).toBe(binding.sha256);}
}
async function assertAssetRegisterNonDocument(page:Page,origin:string,work:AssetUiWork){
 assertAssetRegisterNonDocumentSources();
 await expect(page).toHaveURL(origin+assetRegisterPath); // no detail, print, query or hash mode
 const current=await assetUiWork(page.context(),origin,work.companyId);
 expect({valid:current.valid,companyId:current.companyId,userId:current.userId,assignmentId:current.assignmentId,revision:current.revision}).toEqual({valid:true,companyId:work.companyId,userId:work.userId,assignmentId:work.assignmentId,revision:work.revision});expect(current.options.find(option=>option.id===current.assignmentId)?.role).toBe('holding_owner');
 const stamp=page.locator('[data-work-assignment]');await expect(stamp).toHaveCount(1);await expect(stamp).toHaveAttribute('data-work-stale','false');for(const [key,value] of Object.entries({company:current.companyId,user:current.userId,assignment:current.assignmentId,revision:String(current.revision)}))await expect(stamp).toHaveAttribute('data-work-'+key,value);
 const content=page.locator('#workspace-content:visible'),section=content.locator('section[data-section-workspace]');await expect(section).toHaveCount(1);await expect(section.getByRole('heading',{name:'Assets & maintenance',level:1,exact:true})).toBeVisible();await expect(section.getByRole('search')).toHaveCount(1);await expect(section.getByRole('button',{name:'Register asset',exact:true})).toBeEnabled();
 const data=await assetUiWorkspace(page.context(),origin,current);expect(data.canCreate).toBe(true);expect(data.items.length).toBeGreaterThan(0);const region=section.getByRole('region',{name:'Asset register',exact:true}),table=region.getByRole('table');await expect(region).toHaveAttribute('data-slot','table-frame');await expect(table).toHaveCount(1);await expect(table.getByRole('columnheader')).toHaveCount(4);expect((await table.getByRole('columnheader').allTextContents()).map(text=>text.trim())).toEqual(['Asset','Status','Condition','Details']);
 const record=data.items[0],link=table.getByRole('link',{name:`${record.code} · ${record.name}`,exact:true});await expect(link).toHaveCount(1);await expect(link).toHaveAttribute('data-record-link','true');const href=await link.getAttribute('href');expect(href).toBeTruthy();if(!href)throw Error('NATIVE_ASSET_LIST_RECORD_LINK_REQUIRED');const target=new URL(href,page.url());expect(target.origin).toBe(origin);expect(target.pathname).toBe(`${assetRegisterPath}/${encodeURIComponent(record.id)}/overview`);expect(target.searchParams.get('returnTo')).toBe(assetRegisterPath);
 // Absence alone is NOT the applicability proof: these assertions accompany
 // the exact reviewed source hashes, live native Company owner and list shape.
 const documentAction=/\b(?:print|export|save as pdf|download pdf)\b|พิมพ์|ส่งออก|บันทึก.*PDF/i;
 await expect(content.getByRole('button',{name:documentAction})).toHaveCount(0);await expect(content.getByRole('link',{name:documentAction})).toHaveCount(0);await expect(content.locator('a[download],a[href*="/print"],a[href*="print="],a[href*="/export"],a[href$=".pdf"],button[formaction*="print"],iframe,object,embed,[data-print-document],[data-document-preview],.print-page,.print-only')).toHaveCount(0);await expect(page.getByRole('dialog').filter({visible:true})).toHaveCount(0);
 return {schema:'erpx-asset-register-document-applicability/v1',condition:'print if document',document:false,routeSource,route:assetRegisterPath,owner:'assets',presentation:'native-record-register',sourceBindings:assetRegisterNonDocumentSources,asserted:['exact-canonical-list-no-detail-mode','live-native-Company-user-assignment-revision','native-owned-list-record-link','exact-register-table-and-create-control','no-document-preview-or-print-export-control']} as const;
}

export async function assetUiWork(context:BrowserContext,origin:string,companyId:string){
 const response=await context.request.get(origin+roles,{headers:{'x-erpx-context-company':companyId}});
 try{expect(response.status()).toBe(200);const {data}=await response.json() as {data:AssetUiWork};expect(data.companyId).toBe(companyId);expect(data.userId).toEqual(expect.any(String));expect(data.assignmentId).toEqual(expect.any(String));expect(Number.isSafeInteger(data.revision)&&data.revision>0).toBe(true);return data;}finally{await response.dispose();}
}
export function assetUiHeaders(work:AssetUiWork){return {'x-erpx-context-company':work.companyId,'x-erpx-context-assignment':work.assignmentId,'x-erpx-context-revision':String(work.revision)};}
export async function assetUiWorkspace(context:BrowserContext,origin:string,work:AssetUiWork){
 const response=await context.request.get(origin+endpoint,{headers:assetUiHeaders(work)});
 try{expect(response.status()).toBe(200);const {data}=await response.json() as {data:AssetUiData};expect(Array.isArray(data.items)&&Array.isArray(data.nodes)).toBe(true);return data;}finally{await response.dispose();}
}
/** Uses the original runner's case artifact directory and the actual retained
 * Playwright attachment copy. Browser-runner seals the original leaves later. */
export async function assetUiCapture(page:Page,info:TestInfo,origin:string){
 const run=process.env.ERPX_BROWSER_SIMULATION_RUN??process.env.ERPX_DEMO_RUN;
 if(!run||!/^[a-f0-9]{12}$/.test(run))throw Error('EXACT_ASSET_UI_RUN_REQUIRED');
 const statePath=resolve(root,'.runtime/browser',run,'state.json'),stateBytes=readFileSync(statePath),state=JSON.parse(stateBytes.toString()) as {run:string;prefix:string;origin:string;port:number;ready:boolean;diagnostic?:boolean;pid:number;jobsPid:number;jobsPaused?:boolean;sourceFingerprint:string;softwareVersion:string;webEntrypoint:string;jobsEntrypoint:string;productHosts?:boolean;release:{id:string;fileManifestSha256:string};fixture:{companyId:string}};
 expect(state).toMatchObject({run,prefix:`erpx_browser_${run}`,origin:'http://127.0.0.1:3104',port:3104,ready:true});expect(state.diagnostic).toBeFalsy();expect(state.jobsPaused).not.toBe(true);expect(state.pid).toBeGreaterThan(0);expect(state.jobsPid).toBeGreaterThan(0);
 expect(state.webEntrypoint).toBe(resolve(root,'.runtime/browser',run,'release/server.js'));expect(state.jobsEntrypoint).toBe(resolve(root,'.runtime/browser',run,'release/tools/jobs.mjs'));
 const sourceFingerprint=await runtimeSourceFingerprint(root),verificationFingerprint=await verificationSourceFingerprint(root,sourceFingerprint),candidate=execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim();
 expect(candidate).toMatch(/^[a-f0-9]{40}$/);expect(state.sourceFingerprint).toBe(sourceFingerprint);expect(state.softwareVersion).toBe(`release-${sourceFingerprint}`);
 const releaseDirectory=resolve(root,'.runtime/releases',state.release.id),release=JSON.parse(readFileSync(resolve(releaseDirectory,'release.json'),'utf8')) as {id:string;sourceFingerprint:string;fileManifestSha256:string;node:string;postgres:string};
 expect(realpathSync.native(releaseDirectory)).toBe(releaseDirectory);expect(release).toMatchObject({...state.release,sourceFingerprint,node:'26.11.1',postgres:'18.6'});expect(hash(readFileSync(resolve(releaseDirectory,'release-files.json')))).toBe(release.fileManifestSha256);
 const leasePath=process.env.ERPX_PRODUCT_HOST_LEASE,leaseBytes=leasePath?readFileSync(leasePath):null;
 if(origin!==state.origin){if(!leasePath||origin!=='https://assets-dev-erpx.jesadakorn.com')throw Error('ACTUAL_ASSETS_PRODUCT_HOST_LEASE_REQUIRED');const lease=await readProductHostLease(root,leasePath);expect(state.productHosts).toBe(true);expect(lease.run).toBe(run);expect(lease.sourceFingerprint).toBe(sourceFingerprint);expect(lease.verificationFingerprint).toBe(verificationFingerprint);expect(lease.release).toEqual(state.release);expect(lease.origins).toContain(origin);}else expect(state.productHosts).not.toBe(true);
 const coveragePath=resolve(root,'docs/ux-ui/standard-coverage.json'),coverageBytes=readFileSync(coveragePath),coverage=JSON.parse(coverageBytes.toString()) as {closurePaths:string[];sources:{source:string;fingerprint:string;surfaces:Surface[]}[];routes:{source:string;route:string;fingerprint:string;closureIds:number[];styles:string[];dependencies:string[];routeContext:{kind:string}}[]},routesFound=coverage.routes.filter(row=>row.source===routeSource),sourcesFound=coverage.sources.filter(row=>row.source===targetSource);
 expect(routesFound).toHaveLength(1);expect(sourcesFound).toHaveLength(1);const route=routesFound[0],source=sourcesFound[0],closure=route.closureIds.map(index=>coverage.closurePaths[index]);expect(route.route).toBe('/holdings/[holdingSlug]/services/assets/register');expect(route.routeContext.kind).toBe('children');expect(route.dependencies).toContain(targetSource);expect(closure).toContain(routeSource);expect(source.fingerprint).toBe(hash(readFileSync(resolve(root,targetSource))));expect(route.fingerprint).toBe(hash([...closure,...route.styles].map(path=>`${path}:${hash(readFileSync(resolve(root,path)))}`).sort().join('\n')));
 const surface=(component:string,condition?:string)=>{const found=source.surfaces.filter(row=>row.component===component&&(condition===undefined||row.conditions.includes(condition)));expect(found).toHaveLength(1);const row=found[0];return `surface:${targetSource}:${hash(JSON.stringify([row.component,row.line,row.props,row.conditions]))}`;};
 const drawerSurface=surface('DraftFormDrawer'),errorSurface=surface('InlineNotice','error'),historySurface=surface('Dialog');
 return {run,companyId:state.fixture.companyId,drawerSurface,errorSurface,historySurface,recoveryNoticeSurface:surface('InlineNotice','recovery?.notice'),recoveryUncertainSurface:surface('InlineNotice','recovery?.uncertain'),
  async observe(label:string,checks:string[],work:AssetUiWork,authorityBranch='allowed:actual-native-Company-holding_owner'){
   expect(new URL(page.url()).origin).toBe(origin);expect(new URL(page.url()).pathname).toBe(assetRegisterPath);expect(work.companyId).toBe(state.fixture.companyId);expect(new Set(checks).size).toBe(checks.length);const applicability=checks.includes('print if document')?await assertAssetRegisterNonDocument(page,origin,work):undefined;
   const viewport=page.viewportSize();if(!viewport)throw Error('ACTUAL_ASSET_UI_VIEWPORT_REQUIRED');expect([320,390,768,1024,1440]).toContain(viewport.width);expect(viewport.height).toBe(900);expect(await page.evaluate(()=>devicePixelRatio)).toBe(1);
   const theme=await page.locator('html').getAttribute('data-theme');expect(['light','dark','system']).toContain(theme);const resolvedTheme=await page.locator('html').evaluate(element=>getComputedStyle(element).colorScheme);expect(['light','dark']).toContain(resolvedTheme);if(theme!=='system')expect(resolvedTheme).toBe(theme);
   const imagePath=info.outputPath(label+'.png'),output=relative(resolve(root,'.scratch/dev/test-suites/browser'),imagePath).split('/');expect(output.length).toBeGreaterThan(3);expect(output[1]).toBe(run);expect(output[2]).toBe('artifacts');expect(output.every(part=>part&&part!=='.'&&part!=='..')).toBe(true);expect(realpathSync.native(dirname(imagePath))).toBe(dirname(imagePath));expect(existsSync(imagePath)).toBe(false);
   const image=await page.screenshot({path:imagePath,type:'png',fullPage:false,scale:'css'});expect(image.readUInt32BE(16)).toBe(viewport.width);expect(image.readUInt32BE(20)).toBe(viewport.height);
   const index=info.attachments.length;await info.attach('asset-owner-state-image',{path:imagePath,contentType:'image/png'});expect(info.attachments).toHaveLength(index+1);const attachment=info.attachments[index];expect(attachment).toMatchObject({name:'asset-owner-state-image',contentType:'image/png'});if(!attachment.path)throw Error('ORIGINAL_ASSET_UI_IMAGE_ATTACHMENT_REQUIRED');const retained=readFileSync(attachment.path);expect(retained.equals(image)).toBe(true);expect(attachment.path.startsWith(dirname(imagePath)+'/')).toBe(true);expect(realpathSync.native(attachment.path)).toBe(attachment.path);expect(lstatSync(attachment.path).isFile()).toBe(true);
   const screenshot={path:relative(root,attachment.path).replaceAll('\\','/'),sha256:hash(retained),bytes:retained.length},ownerContext=JSON.stringify({companyId:work.companyId,userId:work.userId,assignmentId:work.assignmentId,revision:work.revision});
   for(const target of [{kind:'route',source:routeSource,fingerprint:route.fingerprint},{kind:'source',source:targetSource,fingerprint:source.fingerprint}]){const path=info.outputPath(label+'-'+target.kind+'.json');expect(path.startsWith(dirname(imagePath)+'/')).toBe(true);writeFileSync(path,JSON.stringify({schema:'erpx-ui-browser-observation/v1',result:'passed',run,candidate,sourceFingerprint,verificationFingerprint,releaseId:release.id,releaseManifestSha256:release.fileManifestSha256,targetSource:target.source,targetKind:target.kind,targetFingerprint:target.fingerprint,routeSource,url:page.url(),ownerContext,authorityBranch,authentication:'normal-native-owner',viewport,deviceScaleFactor:1,theme,resolvedTheme,checks,screenshot,...(applicability?{documentApplicability:applicability}:{})},null,2)+'\n',{flag:'wx',mode:0o600});await info.attach('erpx-ui-observation',{path,contentType:'application/json'});}
  },
  async stable(){assertAssetRegisterNonDocumentSources();expect(readFileSync(statePath).equals(stateBytes)).toBe(true);expect(readFileSync(coveragePath).equals(coverageBytes)).toBe(true);if(leasePath&&leaseBytes)expect(readFileSync(leasePath).equals(leaseBytes)).toBe(true);expect(await runtimeSourceFingerprint(root)).toBe(sourceFingerprint);expect(await verificationSourceFingerprint(root,sourceFingerprint)).toBe(verificationFingerprint);}
 };
}
export async function assetUiFill(form:Locator,code:string,name:string,category:string,today:string){
 await form.getByRole('textbox',{name:'Asset code',exact:true}).fill(code);await form.getByRole('textbox',{name:'Name',exact:true}).fill(name);await form.getByRole('textbox',{name:'Category',exact:true}).fill(category);await form.getByRole('textbox',{name:'Currency',exact:true}).fill('THB');await form.getByRole('textbox',{name:'Acquisition cost',exact:true}).fill('123456.78');await form.locator('input[name="acquiredDate"]').fill(today);
}
export async function assetUiDiscard(page:Page){
 const drawer=page.getByRole('dialog',{name:'Register asset',exact:true});await drawer.getByRole('button',{name:'Close dialog',exact:true}).click();const confirmation=page.getByRole('dialog',{name:'Discard changes?',exact:true});await expect(confirmation).toBeVisible();await confirmation.getByRole('button',{name:'Discard draft',exact:true}).click();await expect(drawer).not.toBeVisible();
}

/** HTTPS producer uses ONLY the already-owned Account→Apps→Assets browser
 * session. The existing edge remains read-only for all business commands. */
export async function observeAssetProductHost(page:Page,context:BrowserContext,info:TestInfo,companyId:string,userId:string){
 const origin='https://assets-dev-erpx.jesadakorn.com',apps='https://apps-dev-erpx.jesadakorn.com',proof=await assetUiCapture(page,info,origin),thai='แบบร่างทะเบียนทรัพย์สินสำหรับเจ้าหน้าที่ผู้รับผิดชอบการตรวจสอบความปลอดภัยและการบำรุงรักษา '.repeat(2).slice(0,150);
 expect(companyId).toBe(proof.companyId);let posts=0;page.on('request',request=>{if(request.method()==='POST'&&new URL(request.url()).origin===origin&&new URL(request.url()).pathname===endpoint)posts++;});
 for(const [theme,scheme] of [['light','light'],['dark','dark'],['system','light'],['system','dark']] as const){
  await context.addCookies([{name:'erpx_appearance',value:theme,url:origin}]);await page.emulateMedia({colorScheme:scheme});
  for(const width of [320,390,768,1024,1440]){
   const label=`asset-product-${width}-${theme}-${scheme}`;await page.setViewportSize({width,height:900});await page.goto(origin+assetRegisterPath);await expect(page).toHaveURL(origin+assetRegisterPath);const work=await assetUiWork(context,origin,companyId);expect(work.valid).toBe(true);expect(work.userId).toBe(userId);expect(work.options.find(option=>option.id===work.assignmentId)?.role).toBe('holding_owner');
   const stamp=page.locator('[data-work-assignment]');await expect(stamp).toHaveCount(1);await expect(stamp).toHaveAttribute('data-work-stale','false');for(const [key,value] of Object.entries({company:work.companyId,user:work.userId,assignment:work.assignmentId,revision:String(work.revision)}))await expect(stamp).toHaveAttribute('data-work-'+key,value);
   const content=page.locator('#workspace-content:visible');await expect(content.getByRole('heading',{name:'Assets & maintenance',level:1,exact:true})).toBeVisible();await expect(page.locator('html')).toHaveAttribute('data-theme',theme);expect(await page.locator('html').evaluate(element=>getComputedStyle(element).colorScheme)).toBe(scheme);
   const search=content.getByRole('searchbox',{name:'Search this view',exact:true}),searchForm=content.getByRole('search'),missing=`ui-missing-${proof.run}-${width}-${theme}-${scheme}`;await search.fill(missing);await searchForm.getByRole('button',{name:'Search',exact:true}).click();await expect(page).toHaveURL(url=>url.origin===origin&&url.pathname===assetRegisterPath&&url.searchParams.get('q')===missing);await expect(content.getByRole('table').locator('tbody tr')).toHaveCount(0);await expect(content.getByText('No records match this view in your current scope.',{exact:true})).toBeVisible();await proof.observe(label+'-empty',['loaded','empty'],work);await content.getByRole('link',{name:'Clear filters',exact:true}).click();await expect(search).toHaveValue('');await expect(page).toHaveURL(origin+assetRegisterPath);await proof.observe(label+'-filters',['pagination/search/reset','print if document'],work);
   const opener=content.getByRole('button',{name:'Register asset',exact:true});await expect(opener).toBeEnabled();await opener.focus();await expect(opener).toBeFocused();await page.keyboard.press('Enter');const drawer=page.getByRole('dialog',{name:'Register asset',exact:true}),form=drawer.locator('form.ui-form-grid'),branch=form.getByRole('combobox',{name:'Branch',exact:true}),name=form.getByRole('textbox',{name:'Name',exact:true});await expect(drawer).toBeVisible();await expect(drawer).toHaveAttribute('data-presentation','form-drawer');await expect(branch).toBeFocused();expect(await drawer.evaluate(element=>!document.querySelector('#workspace-content')?.contains(element))).toBe(true);expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1)).toBe(true);
   const controls=drawer.locator('button:not([disabled]),a[href],input:not([type="hidden"]):not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex="0"]:not([disabled])').filter({visible:true});expect(await controls.count()).toBeGreaterThan(1);await controls.last().focus();await page.keyboard.press('Tab');await expect(controls.first()).toBeFocused();await controls.first().focus();await page.keyboard.press('Shift+Tab');await expect(controls.last()).toBeFocused();await page.keyboard.press('Escape');await expect(drawer).not.toBeVisible();await expect(opener).toBeFocused();await page.keyboard.press('Enter');await expect(branch).toBeFocused();await proof.observe(label+'-focus',['keyboard','focus','escape','portalled popup/draft focus',proof.drawerSurface],work);
   // Native required validation stays entirely in the actual browser. No POST
   // to the read-only edge is attempted or relabelled as a native owner error.
   await form.getByRole('button',{name:'Register asset',exact:true}).click();await expect(form.locator('input[name="code"]')).toBeFocused();expect(await form.evaluate(element=>!(element as HTMLFormElement).checkValidity())).toBe(true);expect(posts).toBe(0);await proof.observe(label+'-validation',['validation'],work);await name.fill(thai);await expect(name).toHaveValue(thai);await proof.observe(label+'-thai',['long-thai'],work);
   // A real address-bar product navigation encounters the browser's native
   // dirty guard while the modal correctly makes background Apps links inert.
   const staying=page.url(),warning=page.waitForEvent('dialog'),navigation=page.goto(apps+'/',{waitUntil:'domcontentloaded'}).then(()=>null,error=>error),dialog=await warning;expect(dialog.type()).toBe('beforeunload');await dialog.dismiss();await navigation;await expect(page).toHaveURL(staying);await expect(name).toHaveValue(thai);await proof.observe(label+'-dirty-product',['dirty-reload'],work);await assetUiDiscard(page);
   // Now use the actual rendered product-switch control and the native Apps
   // SSO tile; no direct product cookie/session or business authority is made.
   const appsLink=page.locator('a[data-product-switch]').filter({hasText:/^Apps$/});await expect(appsLink).toHaveCount(1);await expect(appsLink).toBeVisible();await expect(appsLink).toBeInViewport();await appsLink.click();await expect(page).toHaveURL(url=>url.origin===apps);let choices:Locator[]=[];
   await expect.poll(async()=>{choices=[];for(const anchor of await page.locator('a[data-product-switch]').all()){const href=await anchor.getAttribute('href');if(!href)continue;const url=new URL(href,page.url()),next=url.searchParams.get('next');if(url.origin!==origin||url.pathname!=='/sso/start'||!next)continue;const target=new URL(next,origin);if(target.pathname==='/launch'&&target.searchParams.get('companyId')===companyId&&!target.searchParams.get('next')?.includes('/branches/'))choices.push(anchor);}return choices.length;}).toBe(1);await choices[0].click();await page.waitForURL(url=>url.origin===origin&&(url.pathname==='/choose-work-role'||url.pathname.startsWith('/holdings/')));if(new URL(page.url()).pathname==='/choose-work-role'){const choice=page.locator('form[action="/launch"]').filter({hasText:/holding owner/i});await expect(choice).toHaveCount(1);await choice.locator('button[type="submit"]').click();}await expect(page).toHaveURL(url=>url.origin===origin&&url.pathname.startsWith('/holdings/simulation/'));await page.goto(origin+assetRegisterPath);const returned=await assetUiWork(context,origin,companyId);expect(returned.valid).toBe(true);expect(returned.userId).toBe(userId);expect(returned.options.find(option=>option.id===returned.assignmentId)?.role).toBe('holding_owner');await expect(page.locator('[data-work-assignment]')).toHaveAttribute('data-work-assignment',returned.assignmentId);await expect(page.locator('[data-work-assignment]')).toHaveAttribute('data-work-revision',String(returned.revision));await expect(opener).toBeEnabled();await proof.observe(label+'-product-roundtrip',['product-switch'],returned);
  }
 }
 expect(posts).toBe(0);await proof.stable();
}
