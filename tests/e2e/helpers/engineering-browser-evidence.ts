import type {Browser,BrowserContext,BrowserContextOptions,Page,TestInfo} from '@playwright/test';
import {mkdir} from 'node:fs/promises';
import {maskEngineeringSecurityEvidence} from './engineering-browser-privacy';

/** Multi-persona contexts do not use Playwright's context fixture/video recorder. */
export async function createEngineeringBrowserContext(browser:Browser,info:TestInfo,overrides:BrowserContextOptions={}):Promise<BrowserContext>{
 const record=process.env.ERPX_ENGINEERING_BROWSER_EVIDENCE==='1',directory=info.outputPath('persona-recordings');
 if(record)await mkdir(directory,{recursive:true,mode:0o700});
 const options={...info.project.use,...overrides},viewport=options.viewport??{width:1280,height:800};
 const context=await browser.newContext({...options,...(record?{recordVideo:{dir:directory,size:viewport}}:{})});
 // Diagnostic replays report a missing control promptly; final runs keep their ordinary timeouts.
 if(process.env.ERPX_ENGINEERING_DIAGNOSTIC_FAST==='1'){
  context.setDefaultTimeout(15_000);
  context.setDefaultNavigationTimeout(30_000);
 }
 if(!record)return context;
 await maskEngineeringSecurityEvidence(context);
 const pages:Page[]=[];context.on('page',page=>pages.push(page));
 const close=context.close.bind(context);let ended=false;
 context.close=async options=>{
  if(ended)return;ended=true;
  let screenshotFailed=false;
  for(const [index,page] of pages.entries())if(!page.isClosed()){
   try{await info.attach(`persona-final-screen-${index+1}`,{body:await page.screenshot({fullPage:true,animations:'disabled',timeout:10_000}),contentType:'image/png'});}
   catch{screenshotFailed=true;}
  }
  await close(options);
  // Closing flushes WebM bytes. Await attachments so the JSON runner report
  // and packaging checksum refer to the actual persisted workflow recordings.
  for(const [index,page] of pages.entries()){
   const video=page.video();if(!video)throw new Error('ENGINEERING_PERSONA_VIDEO_REQUIRED');
   await info.attach(`persona-video-${index+1}`,{path:await video.path(),contentType:'video/webm'});
  }
  if(screenshotFailed)throw new Error('ENGINEERING_PERSONA_SCREENSHOT_REQUIRED');
 };
 return context;
}
