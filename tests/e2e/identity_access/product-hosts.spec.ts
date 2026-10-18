import {expect,test} from '@playwright/test';
import {randomUUID} from 'node:crypto';
import {productIds} from '../../../src/lib/commercial/product-registry';
import {verifyProductHost} from '../../support/browser/product-host-session';
import {workRequestDraftProductHostCase} from '../../../scripts/testing/product-host-cohort';

// Missing owned mode is a failure, never an accepted skip/Console fallback.
for(const product of productIds)test(`isolated ${product} Account Apps SSO reaches its actual owner landing and API`,async({browser},testInfo)=>{
 if(!process.env.ERPX_PRODUCT_HOST_LEASE)throw Error('ARMED_OFFICIAL_PRODUCT_HOST_MODE_REQUIRED');
 await verifyProductHost(browser,testInfo,product);
});

import {observeAssetProductHost} from '../../support/browser/asset-ui-observation';

test('isolated Assets native drawer and dirty product switch observations across the appearance matrix',async({browser},testInfo)=>{
 test.setTimeout(360_000);
 if(!process.env.ERPX_PRODUCT_HOST_LEASE)throw Error('ARMED_OFFICIAL_PRODUCT_HOST_MODE_REQUIRED');
 await verifyProductHost(browser,testInfo,'assets',async({page,context,origin,companyId,userId})=>{
  if(origin!=='https://assets-dev-erpx.jesadakorn.com')throw Error('EXACT_ASSETS_PRODUCT_ORIGIN_REQUIRED');
  await observeAssetProductHost(page,context,testInfo,companyId,userId);
 });
});


import {observeFinanceInvoiceDrawer} from '../../support/browser/finance-invoice-ui-observation';

test('isolated Finance native invoice drawer keeps dirty work across the appearance matrix',async({browser},testInfo)=>{
 test.setTimeout(600_000);
 if(!process.env.ERPX_PRODUCT_HOST_LEASE)throw Error('ARMED_OFFICIAL_PRODUCT_HOST_MODE_REQUIRED');
 await verifyProductHost(browser,testInfo,'finance',async({page,context,origin,companyId,userId})=>{
  if(origin!=='https://finance-dev-erpx.jesadakorn.com')throw Error('EXACT_FINANCE_PRODUCT_ORIGIN_REQUIRED');
  await observeFinanceInvoiceDrawer(page,context,testInfo,origin,companyId,userId);
 });
});

test(workRequestDraftProductHostCase,async({browser},testInfo)=>{
 test.setTimeout(240_000);if(!process.env.ERPX_PRODUCT_HOST_LEASE)throw Error('ARMED_OFFICIAL_PRODUCT_HOST_MODE_REQUIRED');
 await verifyProductHost(browser,testInfo,'work',async({page,context,origin,companyId,scopedBusinessWorkflow})=>{
  expect(origin).toBe('https://work-dev-erpx.jesadakorn.com');expect(scopedBusinessWorkflow).toBe('work.request-draft');
  const title=`Migration Work draft ${randomUUID()}`,description='A service request created and cancelled through the isolated Work product host.';
  const supportOrigin='https://support-dev-erpx.jesadakorn.com',workPath='/holdings/simulation/work/actions',requestsPath='/api/v1/holdings/simulation/requests',requestApi=(host:string,id:string)=>host+requestsPath+'/'+encodeURIComponent(id),workUrl=()=>origin+workPath+`?queue=all&type=service_request&q=${encodeURIComponent(title)}`;
  const keys:string[]=[];let requestId:string|undefined;
  const markIsolatedWorkflow=async(route:import('@playwright/test').Route)=>{
   const request=route.request(),url=new URL(request.url());if(request.method()!=='POST')return route.continue();
   const allowed=url.pathname===requestsPath||url.pathname===requestsPath+'/preview'||/^\/api\/v1\/holdings\/simulation\/requests\/[A-Za-z0-9_-]{1,180}\/actions$/.test(url.pathname);if(!allowed||![origin,supportOrigin].includes(url.origin))return route.continue();
   const headers={...request.headers(),'x-erpx-isolated-product-workflow':'work.request-draft'};
   if(url.origin===origin&&url.pathname===requestsPath){keys.push(request.headers()['idempotency-key']??'');if(keys.length===1)return route.fulfill({status:503,contentType:'application/json',body:JSON.stringify({error:{message:'Isolated Work host retry check'}})});}
   return route.continue({headers});
  };
  await page.route(origin+requestsPath+'**',markIsolatedWorkflow);await page.route(supportOrigin+requestsPath+'**',markIsolatedWorkflow);
  const cancelDraft=async(id:string)=>{await page.goto(workUrl(),{waitUntil:'domcontentloaded'});const row=page.getByRole('region',{name:'Work results'}).getByRole('link',{name:new RegExp(title)});await expect(row).toHaveCount(1);await row.click();const response=page.waitForResponse(reply=>reply.url()===requestApi(supportOrigin,id)+'/actions'&&reply.request().method()==='POST');await page.getByRole('button',{name:'Cancel draft',exact:true}).click();const cancelled=await response;expect(cancelled.ok(),await cancelled.text()).toBe(true);};
  try{
   await page.goto(workUrl(),{waitUntil:'domcontentloaded'});await expect(page.getByRole('heading',{name:'My work',exact:true})).toBeVisible();await page.getByRole('button',{name:'New',exact:true}).click();await page.getByRole('dialog',{name:'New work'}).getByRole('link',{name:'Service request',exact:true}).click();
   await expect(page.getByRole('dialog',{name:'Service request',exact:true})).toBeVisible();await page.getByRole('textbox',{name:'Title',exact:true}).fill(title);await page.getByRole('textbox',{name:'Describe the issue',exact:true}).fill(description);
   await page.getByRole('button',{name:'Save draft',exact:true}).click();await expect(page.getByRole('alert').filter({hasText:'Isolated Work host retry check'})).toBeVisible();await expect(page.getByRole('textbox',{name:'Title',exact:true})).toHaveValue(title);const savedResponse=page.waitForResponse(reply=>reply.url()===origin+requestsPath&&reply.request().method()==='POST');await page.getByRole('button',{name:'Save draft',exact:true}).click();const saved=await savedResponse;expect(saved.ok(),await saved.text()).toBe(true);const result=(await saved.json()).data;requestId=String(result.id);expect(requestId).toBeTruthy();expect(keys).toHaveLength(2);expect(keys[0]).toBeTruthy();expect(keys[1]).toBe(keys[0]);expect(requestId).toBe(`request-${keys[0]}`);await expect(page.getByRole('textbox',{name:'Title',exact:true})).toHaveValue(title);
   const persisted=await context.request.get(requestApi(origin,requestId));try{expect(persisted.status()).toBe(200);expect((await persisted.json()).data.request).toMatchObject({title,description,state:'draft'});}finally{await persisted.dispose();}
   await cancelDraft(requestId);await page.reload({waitUntil:'domcontentloaded'});await expect(page.getByRole('button',{name:'Save draft',exact:true})).toHaveCount(0);const cancelled=await context.request.get(requestApi(origin,requestId));let status:string;try{expect(cancelled.status()).toBe(200);status=String((await cancelled.json()).data.request.state);expect(status).toBe('cancelled');}finally{await cancelled.dispose();}
   await testInfo.attach('work-product-host-request-lifecycle',{body:Buffer.from(JSON.stringify({schema:'erpx-product-host-work-request/v1',origin,companyId,requestId,title,description,idempotencyKeys:keys,retryRetainedOriginalKey:keys[0]===keys[1],state:status,isolatedBusinessWorkflow:scopedBusinessWorkflow,wholeProductAcceptance:false})),contentType:'application/json'});
  }finally{
   if(requestId){const current=await context.request.get(requestApi(origin,requestId));let state:string|undefined;try{if(current.ok())state=String((await current.json()).data.request.state);}finally{await current.dispose();}if(state==='draft')await cancelDraft(requestId);}
  }
 });
});
