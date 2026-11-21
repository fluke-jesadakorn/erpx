import { afterEach, beforeAll, expect, it, vi } from "vitest";
import * as https from "@erpx/server-runtime/adapters/runtime/https";
import { generateProviderJson, providerRequest } from "@erpx/server-agent/public/ai-transport";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  reserveAiGatewayRequest, finishAiGatewayRequest, readAiGatewayPolicy,
  type AiGatewayScope,
} from "@erpx/server-agent/public/ai-gateway";
import { billingMonth, monthBounds } from "@erpx/feature-billing/lib/billing";

const db=env.H001_DB;
afterEach(()=>{vi.unstubAllGlobals();vi.restoreAllMocks();});
const holdingId="gateway-test-holding";
const scope=(fundingSource:"erpx"|"byok",id=crypto.randomUUID()):AiGatewayScope=>({
  db,holdingId,companyId:"gateway-company",branchId:"gateway-branch",connectionId:fundingSource==="byok"?"customer-key":null,
  fundingSource,feature:"test",requestId:id,usageId:id,
});
const rows=async()=>{const{start,end}=monthBounds(billingMonth(Date.now()));return (await db.prepare(`SELECT r.id,r.usage_id,r.holding_id,r.company_id,r.branch_id,r.connection_id,r.funding_source,r.provider,r.model,r.feature,r.status,
  r.input_tokens,r.output_tokens,r.units_json::text units_json,r.created_at,r.completed_at,
  e.provider_statement_id,e.currency,e.amount_minor,e.thb_satang,e.verified_at,e.reconciled_at
  FROM ai_gateway_requests r LEFT JOIN ai_provider_cost_evidence e ON e.request_id=r.id
  WHERE r.created_at>=? AND r.created_at<? ORDER BY r.created_at,r.id`).bind(start,end).all<Record<string,unknown>>()).results;};
beforeAll(async()=>{
  await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);
  await db.prepare("INSERT INTO beta_bootstrap(singleton,holding_id,owner_user_id,created_at) VALUES(1,?,?,?) ON CONFLICT(singleton) DO NOTHING")
    .bind(holdingId,"gateway-owner",Date.now()).run();
});

it("reserves an immutable scoped request once and projects only safe metadata",async()=>{
  const request=scope("erpx");
  await reserveAiGatewayRequest(request,"api.provider.example","model-1");
  await expect(reserveAiGatewayRequest(request,"api.provider.example","model-1")).rejects.toMatchObject({code:"AI_GATEWAY_BLOCKED"});
  await finishAiGatewayRequest(request,"succeeded",12,4,{input_tokens:12,output_tokens:4,cache_read_input_tokens:3});
  const found=(await rows()).find(row=>row.id===request.requestId);
  expect(found).toMatchObject({holding_id:holdingId,company_id:"gateway-company",branch_id:"gateway-branch",usage_id:request.requestId,status:"succeeded",input_tokens:12,output_tokens:4,funding_source:"erpx"});
  expect(JSON.stringify(await rows())).not.toContain("api-key");
});

it("accepts a registered company scope when the Holding has no beta bootstrap row",async()=>{
  const companyId=`gateway-company-${crypto.randomUUID()}`;
  await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,?,?)")
    .bind(companyId,companyId,"Gateway company",companyId).run();
  const request={...scope("byok"),holdingId:"h001",companyId};
  await reserveAiGatewayRequest(request,"api.provider.example","model-1");
  await finishAiGatewayRequest(request,"succeeded",1,1,{input_tokens:1,output_tokens:1});
  expect((await rows()).find(row=>row.id===request.requestId))
    .toMatchObject({holding_id:"h001",company_id:companyId,status:"succeeded"});
  await expect(reserveAiGatewayRequest({...request,requestId:crypto.randomUUID(),companyId:"other-company"},"api.provider.example","model-1"))
    .rejects.toMatchObject({code:"AI_GATEWAY_BLOCKED"});
});

it("routes provider I/O through the gateway and captures additional provider units",async()=>{
  const request=scope("erpx");
  const endpoint={baseUrl:"https://api.provider.com/v1",model:"model-1",apiKey:"private",gateway:request};
  await expect(providerRequest(endpoint,"chat/completions",{})).rejects.toMatchObject({code:"AI_GATEWAY_REQUIRED"});
  vi.stubGlobal("fetch",vi.fn(async()=>Response.json({Status:0,Answer:[{type:1,data:"8.8.8.8"}]})));
  const provider=vi.spyOn(https,"pinnedNodeRequest").mockResolvedValue(Response.json({choices:[{finish_reason:"stop",message:{content:'{"ok":true}'}}],usage:{prompt_tokens:6,completion_tokens:2,audio_seconds:3}}));
  expect(await generateProviderJson(endpoint,[{role:"user",content:"Return JSON"}])).toEqual({result:{ok:true},inputTokens:6,outputTokens:2});
  expect(provider).toHaveBeenCalledTimes(1);
  const row=(await rows()).find(item=>item.id===request.requestId);
  expect(row?.status).toBe("succeeded");
  expect(JSON.parse(String(row!.units_json))).toMatchObject({prompt_tokens:6,completion_tokens:2,audio_seconds:3});
  await expect(generateProviderJson(endpoint,[{role:"user",content:"Return JSON"}])).rejects.toMatchObject({code:"AI_GATEWAY_BLOCKED"});
  expect(provider).toHaveBeenCalledTimes(1);
});

it("marks unusable provider responses failed and leaves them without customer cost",async()=>{
  const request=scope("erpx");
  vi.stubGlobal("fetch",vi.fn(async()=>Response.json({Status:0,Answer:[{type:1,data:"8.8.8.8"}]})));
  vi.spyOn(https,"pinnedNodeRequest").mockResolvedValue(Response.json({choices:[{finish_reason:"length",message:{content:"{}"}}],usage:{prompt_tokens:10,completion_tokens:10}}));
  await expect(generateProviderJson({baseUrl:"https://api.provider.com/v1",model:"model-1",apiKey:"private",gateway:request},[{role:"user",content:"Return JSON"}])).rejects.toMatchObject({code:"AI_INCOMPLETE"});
  const row=(await rows()).find(item=>item.id===request.requestId);
  expect(row).toMatchObject({status:"failed",thb_satang:null});
});

it("blocks the Platform key on kill switch but leaves BYOK available",async()=>{
  const before=await readAiGatewayPolicy(db);
  await db.prepare("UPDATE ai_gateway_policies SET platform_enabled=false,updated_by=?,updated_at=? WHERE singleton=1").bind("platform-admin",Date.now()).run();
  expect(await readAiGatewayPolicy(db)).toMatchObject({platformEnabled:false,monthlyRequestLimit:before.monthlyRequestLimit,monthlySpendCapSatang:before.monthlySpendCapSatang});
  await expect(reserveAiGatewayRequest(scope("erpx"),"api.provider.example","model-1")).rejects.toMatchObject({code:"AI_GATEWAY_BLOCKED"});
  const byok=scope("byok");
  await reserveAiGatewayRequest(byok,"api.provider.example","model-1");
  await finishAiGatewayRequest(byok,"succeeded",3,2,{input_tokens:3,output_tokens:2});
  await db.prepare("UPDATE ai_gateway_policies SET platform_enabled=true,updated_by=?,updated_at=? WHERE singleton=1").bind("platform-admin",Date.now()).run();
});

it("reserves the Holding spend allowance before provider I/O",async()=>{
  await db.prepare("UPDATE ai_gateway_policies SET platform_enabled=true,monthly_request_limit=1000,monthly_spend_cap_satang=5000,reservation_satang=5000,updated_by=?,updated_at=? WHERE singleton=1").bind("platform-admin",Date.now()).run();
  await expect(reserveAiGatewayRequest(scope("erpx"),"api.provider.example","model-1")).rejects.toMatchObject({code:"AI_GATEWAY_BLOCKED"});
  await db.prepare("UPDATE ai_gateway_policies SET platform_enabled=true,monthly_request_limit=1000,monthly_spend_cap_satang=10000000,reservation_satang=1000,updated_by=?,updated_at=? WHERE singleton=1").bind("platform-admin",Date.now()).run();
});
