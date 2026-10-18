import { afterEach, expect, it, vi } from "vitest";
import { createFinanceCommandFetch } from "../../../src/lib/finance/finance-command-fetch";

afterEach(()=>vi.unstubAllGlobals());

it.each(['revision','stale','removed'] as const)('retains the exact Finance attempt and original form revision when the boundary becomes %s during native body decoding',async changed=>{
 const node={isConnected:true,dataset:{workUser:'maker',workRevision:'7',workCompany:'original-company',workAssignment:'finance-role',workStale:'false'}};
 let present=true,edit='original-edit';
 const form={isConnected:true,getAttribute:()=>edit} as unknown as HTMLFormElement;
 const browser=Object.assign(new EventTarget(),{location:{origin:'https://finance-dev-erpx.jesadakorn.com'}}),saved:CustomEvent[]=[];
 browser.addEventListener('erpx:record-command-saved',event=>saved.push(event as CustomEvent));
 vi.stubGlobal('window',browser);vi.stubGlobal('document',{activeElement:{closest:()=>form},querySelector:(selector:string)=>selector==='[data-work-revision]'&&present?node:null});
 let decoded!:()=>void,body!:ReadableStreamDefaultController<Uint8Array>;
 const decoding=new Promise<void>(resolve=>{decoded=resolve;}),stream=new ReadableStream<Uint8Array>({start(controller){body=controller;}});
 class ObservedResponse extends Response{override clone(){const result=super.clone(),json=result.json.bind(result);result.json=()=>{decoded();return json();};return result;}}
 const keys:string[]=[],bodies:unknown[]=[],response={data:{id:'original-command',version:2}};
 vi.stubGlobal('fetch',async(_url:RequestInfo|URL,init?:RequestInit)=>{keys.push(new Headers(init?.headers).get('Idempotency-Key')!);bodies.push(init?.body);if(keys.length===1)throw Error('Lost after commit');return keys.length===2?new ObservedResponse(stream,{headers:{'content-type':'application/json'}}):Response.json(response);});
 const request=createFinanceCommandFetch(),url='/api/v1/holdings/acme/finance/budgets',input={method:'POST',body:'{"expectedVersion":1,"amount":"100.01"}'};
 await expect(request(url,input)).rejects.toThrow('Lost after commit');
 const retry=request(url,input);await decoding;edit='newer-local-edit';
 if(changed==='revision')node.dataset.workRevision='8';else if(changed==='stale')node.dataset.workStale='true';else{node.isConnected=false;present=false;}
 body.enqueue(new TextEncoder().encode(JSON.stringify(response)));body.close();
 await expect(retry).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});expect(saved).toEqual([]);
 node.dataset.workRevision='7';node.dataset.workStale='false';node.isConnected=true;present=true;
 expect(request.hasAttempt(url,input.body)).toBe(true);expect(request.completion(url,input.body)?.formEditVersion).toBe('original-edit');await request(url,input);
 expect(request.hasAttempt(url,input.body)).toBe(false);expect(new Set(keys).size).toBe(1);expect(bodies).toEqual([input.body,input.body,input.body]);
 expect(saved).toHaveLength(1);expect(saved[0].detail.formEditVersion).toBe('original-edit');expect(form.getAttribute('data-erpx-edit-version')).toBe('newer-local-edit');
});

it("reuses the command key after a lost acknowledgement and starts a new command after confirmed success",async()=>{
  const seen:string[]=[], send=vi.fn(async (_url:RequestInfo|URL,init?:RequestInit)=>{
    seen.push(new Headers(init?.headers).get("Idempotency-Key")!);
    if(seen.length===1)throw new Error("Connection lost after server commit");
    if(seen.length===2)return new Response("truncated",{status:200});
    return Response.json({data:{id:"retained-draft"}});
  });
  const request=createFinanceCommandFetch(send),input={method:"POST",body:JSON.stringify({expectedVersion:1,amount:"100.01"})};
  await expect(request("/api/v1/holdings/acme/finance/budgets",input)).rejects.toThrow("Connection lost");
  expect(request.hasAttempt("/api/v1/holdings/acme/finance/budgets",input.body)).toBe(true);
  await request("/api/v1/holdings/acme/finance/budgets",input);
  await request("/api/v1/holdings/acme/finance/budgets",input);
  await request("/api/v1/holdings/acme/finance/budgets",input);
  expect(request.hasAttempt("/api/v1/holdings/acme/finance/budgets",input.body)).toBe(false);
  expect(seen[1]).toBe(seen[0]);expect(seen[2]).toBe(seen[0]);expect(seen[3]).not.toBe(seen[0]);
});

it("separates changed payloads and Company roles even when the caller reuses a proposed key",async()=>{
  const dataset={workUser:"maker",workRevision:"1",workCompany:"company-A",workAssignment:"finance-A"};
  vi.stubGlobal("document",{querySelector:()=>({dataset})});
  const seen:string[]=[],send=vi.fn(async(_url:RequestInfo|URL,init?:RequestInit)=>{seen.push(new Headers(init?.headers).get("Idempotency-Key")!);return Response.json({error:{code:"RETRY"}},{status:503});});
  const request=createFinanceCommandFetch(send),input={method:"POST",body:'{"expectedVersion":1}',headers:{"Idempotency-Key":"proposed"}};
  await request("/finance/review",input);
  await request("/finance/review",{...input,headers:{"Idempotency-Key":"different-retry-proposal"}});
  expect(seen[1]).toBe(seen[0]);
  await request("/finance/review",{...input,body:'{"expectedVersion":2}',headers:{"Idempotency-Key":"new-version"}});
  dataset.workCompany="company-B";dataset.workAssignment="finance-B";
  expect(request.hasAttempt("/finance/review",input.body)).toBe(false);
  await request("/finance/review",{...input,headers:{"Idempotency-Key":"new-company"}});
  expect(seen[2]).not.toBe(seen[0]);expect(seen[3]).not.toBe(seen[0]);expect(seen[3]).not.toBe(seen[2]);
});
