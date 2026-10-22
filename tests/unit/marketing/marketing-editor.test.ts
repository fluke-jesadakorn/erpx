import { describe, expect, it,vi } from "vitest";
import { marketingAssetDraft, marketingAssetEditorSchema } from "@erpx/feature-collaboration/lib/marketing-editor";
import {createPublicMarketingCommand,validPublicMarketingReceipt,publicMarketingAttemptCurrent,publicMarketingAcknowledgementCurrent} from '@erpx/feature-marketing/lib/public-form-command';
import {readBoundedPublicMarketingBody} from '@erpx/server-marketing/public/public-form-body';
import {PublicMarketingForm} from '@erpx/feature-marketing/components/public-marketing-form';
import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';

describe("novice marketing asset editor", () => {
  it("creates a consented public form from readable fields", () => {
    expect(marketingAssetDraft({ kind: "form", name: "Service inquiry", headline: "Tell us what you need",
      summary: "We will contact you.", consentRequired: true })).toEqual({
      kind: "form", name: "Service inquiry", configuration: { consentRequired: true },
      preview: { headline: "Tell us what you need", summary: "We will contact you." },
    });
    expect(marketingAssetEditorSchema.safeParse({ kind: "form", name: "Service inquiry",
      headline: "Tell us what you need", summary: "", consentRequired: false }).success).toBe(false);
  });

  it("retains a review reference and human-readable preview for other assets", () => {
    for (const kind of ["content", "domain", "provider"] as const)
      expect(marketingAssetDraft({ kind, name: "Launch review", reference: "POLICY-2026-1",
        summary: "Ready for independent review" })).toEqual({
        kind, name: "Launch review", configuration: { reference: "POLICY-2026-1" },
        preview: { summary: "Ready for independent review" },
      });
    expect(marketingAssetEditorSchema.safeParse({ kind: "domain", name: "Launch review",
      reference: "", summary: "Ready for independent review" }).success).toBe(false);
  });
});

describe('public Forms exact submission acknowledgement',()=>{
 const context=JSON.stringify(['tenant','reviewed-form',2]),body=JSON.stringify({formVersion:2,contactName:'Declared visitor',brief:'Original consented request'});
 it('renders the actual clean public form and fields on the server without a mount discovery request or premature retry/success notice',()=>{const fetch=vi.spyOn(globalThis,'fetch');try{const html=renderToStaticMarkup(createElement(PublicMarketingForm,{holdingSlug:'tenant',formId:'reviewed-form',formVersion:2}));for(const name of ['name','detail','brief','consent'])expect(html).toContain(`name="${name}"`);expect(html).toContain('data-marketing-public-draft');expect(html).toContain('Send request');expect(html).not.toContain('Thank you');expect(html).not.toContain('Retry submitted request');expect(fetch).not.toHaveBeenCalled();}finally{fetch.mockRestore();}});
 it('retains an identical uncertain body, captured revision and key after a malformed200 instead of declaring success',async()=>{
  const commands=createPublicMarketingCommand(()=> 'exact-public-form-key'),attempt=commands.select(body,context,4,7),requests:Array<{body:string;key:string}>=[];
  await expect(commands.execute(attempt,async captured=>{requests.push({body:captured.body,key:captured.key});return Response.json({});},()=>true)).rejects.toThrow('acknowledgement is incomplete');expect(commands.retry()).toBe(attempt);expect(commands.select(body,context,4,7)).toBe(attempt);
  expect(await commands.execute(attempt,async captured=>{requests.push({body:captured.body,key:captured.key});return Response.json({data:{id:'marketing-candidate-native',status:'pending'}});},()=>true)).toEqual({id:'marketing-candidate-native',status:'pending'});expect(requests).toEqual([{body,key:'exact-public-form-key'},{body,key:'exact-public-form-key'}]);expect(commands.retry()).toBeNull();
 });
 it('does not send newer details under an uncertain key and permits only explicit retry of the original frozen request',async()=>{
  const commands=createPublicMarketingCommand(()=> 'captured-form-key'),attempt=commands.select(body,context,1,3);await expect(commands.execute(attempt,async()=>{throw Error('Response lost after actual submit');},()=>true)).rejects.toThrow('Response lost');expect(Object.isFrozen(attempt)).toBe(true);
  expect(()=>commands.select(body+'changed',context,1,4)).toThrow('newer details are still here');expect(()=>commands.select(body,'another-tenant-form',1,3)).toThrow('original form');expect(()=>commands.select(body,context,2,3)).toThrow('original form');expect(commands.retry()).toBe(attempt);expect(attempt.body).toBe(body);
 });
 it('accepts only the original native pending receipt shape and keeps invalid or truncated responses uncertain',async()=>{
  expect(validPublicMarketingReceipt({id:'marketing-candidate-native',status:'pending'})).toBe(true);for(const value of [null,{},[],{id:'',status:'pending'},{id:'native',status:'active'},{id:12,status:'pending'},{data:{id:'native',status:'pending'}}])expect(validPublicMarketingReceipt(value)).toBe(false);
  const commands=createPublicMarketingCommand(()=> 'truncated-key'),attempt=commands.select(body,context,0,0);await expect(commands.execute(attempt,async()=>new Response('{"data":', {status:200,headers:{'Content-Type':'application/json'}}),()=>true)).rejects.toThrow();expect(commands.retry()).toBe(attempt);
 });
 it('keeps an actual success uncertain if its original context changes while the response is in flight',async()=>{
  const commands=createPublicMarketingCommand(()=> 'stale-context-key'),attempt=commands.select(body,context,3,8);let current=true;
  await expect(commands.execute(attempt,async()=>{current=false;return Response.json({data:{id:'native',status:'pending'}});},()=>current)).rejects.toThrow('context changed');expect(commands.retry()).toBe(attempt);let sends=0;await expect(commands.execute(attempt,async()=>{sends++;return Response.json({data:{id:'native',status:'pending'}});},()=>false)).rejects.toThrow('original form');expect(sends).toBe(0);
 });
 it('clears only the captured unchanged edit acknowledgement while retaining any later contact or consent edits',()=>{
  const attempt=createPublicMarketingCommand(()=> 'ack-key').select(body,context,5,10);expect(publicMarketingAttemptCurrent(attempt,context,5)).toBe(true);expect(publicMarketingAcknowledgementCurrent(attempt,context,5,10)).toBe(true);expect(publicMarketingAcknowledgementCurrent(attempt,context,5,11)).toBe(false);expect(publicMarketingAcknowledgementCurrent(attempt,'other-form',5,10)).toBe(false);expect(publicMarketingAcknowledgementCurrent(attempt,context,6,10)).toBe(false);
 });
 it('releases a definitely refused fresh request for correction while retaining unknown server outcomes',async()=>{
  const commands=createPublicMarketingCommand(()=> 'refused-key'),attempt=commands.select(body,context,0,0);await expect(commands.execute(attempt,async()=>Response.json({error:{message:'Review these fields.'}},{status:422}),()=>true)).rejects.toThrow('Review these fields.');expect(commands.retry()).toBeNull();expect(commands.select(body+'corrected',context,0,1).body).toBe(body+'corrected');await expect(commands.execute(attempt,async()=>Response.json({error:{message:'Unknown server result'}},{status:500}),()=>true)).rejects.toThrow('Unknown server result');expect(commands.retry()).toBe(attempt);
 });
});

describe('public Forms bounded request body',()=>{
 it('rejects a declared oversized body before acquiring a stream reader',async()=>{
  const cancel=vi.fn(async()=>{}),getReader=vi.fn(),request={headers:new Headers({'content-length':String(32*1024+1)}),body:{cancel,getReader}} as unknown as Request;
  await expect(readBoundedPublicMarketingBody(request)).rejects.toMatchObject({status:413,code:'BODY_TOO_LARGE'});
  expect(cancel).toHaveBeenCalledOnce();expect(getReader).not.toHaveBeenCalled();
 });
 it('cancels a chunked body as soon as streamed bytes exceed 32 KiB',async()=>{
  let reads=0,cancelled=false,released=false;
  const reader={read:async()=>{reads++;return reads<=2?{done:false,value:new Uint8Array(reads===1?20*1024:13*1024)}:{done:false,value:new Uint8Array(1024*1024)};},cancel:async()=>{cancelled=true;},releaseLock:()=>{released=true;}};
  const request={headers:new Headers(),body:{getReader:()=>reader}} as unknown as Request;
  await expect(readBoundedPublicMarketingBody(request)).rejects.toMatchObject({status:413,code:'BODY_TOO_LARGE'});
  expect(reads).toBe(2);expect(cancelled).toBe(true);expect(released).toBe(true);
 });
 it('preserves valid UTF-8 form text under the byte budget',async()=>{
  const body=JSON.stringify({brief:'ก'.repeat(9000)}),bytes=new TextEncoder().encode(body),request={headers:new Headers(),body:{getReader:()=>{let sent=false;return {read:async()=>{if(sent)return {done:true,value:undefined};sent=true;return {done:false,value:bytes};},cancel:async()=>{},releaseLock:()=>{}};}}} as unknown as Request;
  expect(await readBoundedPublicMarketingBody(request)).toBe(body);
 });
});
