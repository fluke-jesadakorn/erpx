import { afterEach, expect, it, vi } from "vitest";
import { createFinanceCommandFetch } from "../src/lib/finance-command-fetch";

afterEach(()=>vi.unstubAllGlobals());

it("reuses the command key after a lost acknowledgement and starts a new command after confirmed success",async()=>{
  const seen:string[]=[], send=vi.fn(async (_url:RequestInfo|URL,init?:RequestInit)=>{
    seen.push(new Headers(init?.headers).get("Idempotency-Key")!);
    if(seen.length===1)throw new Error("Connection lost after server commit");
    if(seen.length===2)return new Response("truncated",{status:200});
    return Response.json({data:{id:"retained-draft"}});
  });
  const request=createFinanceCommandFetch(send),input={method:"POST",body:JSON.stringify({expectedVersion:1,amount:"100.01"})};
  await expect(request("/api/v1/holdings/acme/finance/budgets",input)).rejects.toThrow("Connection lost");
  await request("/api/v1/holdings/acme/finance/budgets",input);
  await request("/api/v1/holdings/acme/finance/budgets",input);
  await request("/api/v1/holdings/acme/finance/budgets",input);
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
  await request("/finance/review",{...input,headers:{"Idempotency-Key":"new-company"}});
  expect(seen[2]).not.toBe(seen[0]);expect(seen[3]).not.toBe(seen[0]);expect(seen[3]).not.toBe(seen[2]);
});
