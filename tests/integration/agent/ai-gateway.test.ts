import { afterEach, beforeAll, expect, it, vi } from "vitest";
import * as https from "@erpx/server-runtime/adapters/runtime/https";
import { generateProviderJson, providerRequest } from "@erpx/server-agent/public/ai-transport";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  recordAiProviderStatement, recordVerifiedAiCost, reconcileAiProviderStatement,
  reserveAiGatewayRequest, finishAiGatewayRequest, safeAiGatewayRows, updateAiGatewayPolicy,
  readAiGatewayPolicy, setAiPlatformKeyEnabled,
  type AiGatewayScope,
} from "@erpx/server-agent/public/ai-gateway";
import { billingMonth } from "@erpx/feature-billing/lib/billing";

const db=env.H001_DB;
afterEach(()=>{vi.unstubAllGlobals();vi.restoreAllMocks();});
const holdingId="gateway-test-holding";
const scope=(fundingSource:"erpx"|"byok",id=crypto.randomUUID()):AiGatewayScope=>({
  db,holdingId,companyId:"gateway-company",branchId:"gateway-branch",connectionId:fundingSource==="byok"?"customer-key":null,
  fundingSource,feature:"test",requestId:id,usageId:id,
});
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
  const rows=await safeAiGatewayRows(db,billingMonth(Date.now()));
  expect(rows.find(row=>row.id===request.requestId)).toMatchObject({holding_id:holdingId,company_id:"gateway-company",branch_id:"gateway-branch",usage_id:request.requestId,status:"succeeded",input_tokens:12,output_tokens:4,funding_source:"erpx"});
  expect(JSON.stringify(rows)).not.toContain("api-key");
});

it("accepts a registered company scope when the Holding has no beta bootstrap row",async()=>{
  const companyId=`gateway-company-${crypto.randomUUID()}`;
  await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,?,?)")
    .bind(companyId,companyId,"Gateway company",companyId).run();
  const request={...scope("byok"),holdingId:"h001",companyId};
  await reserveAiGatewayRequest(request,"api.provider.example","model-1");
  await finishAiGatewayRequest(request,"succeeded",1,1,{input_tokens:1,output_tokens:1});
  expect((await safeAiGatewayRows(db,billingMonth(Date.now()))).find(row=>row.id===request.requestId))
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
  const row=(await safeAiGatewayRows(db,billingMonth(Date.now()))).find(item=>item.id===request.requestId);
  expect(row?.status).toBe("succeeded");
  expect(JSON.parse(row!.units_json)).toMatchObject({prompt_tokens:6,completion_tokens:2,audio_seconds:3});
  await expect(generateProviderJson(endpoint,[{role:"user",content:"Return JSON"}])).rejects.toMatchObject({code:"AI_GATEWAY_BLOCKED"});
  expect(provider).toHaveBeenCalledTimes(1);
});

it("marks unusable provider responses failed and leaves them without customer cost",async()=>{
  const request=scope("erpx");
  vi.stubGlobal("fetch",vi.fn(async()=>Response.json({Status:0,Answer:[{type:1,data:"8.8.8.8"}]})));
  vi.spyOn(https,"pinnedNodeRequest").mockResolvedValue(Response.json({choices:[{finish_reason:"length",message:{content:"{}"}}],usage:{prompt_tokens:10,completion_tokens:10}}));
  await expect(generateProviderJson({baseUrl:"https://api.provider.com/v1",model:"model-1",apiKey:"private",gateway:request},[{role:"user",content:"Return JSON"}])).rejects.toMatchObject({code:"AI_INCOMPLETE"});
  const row=(await safeAiGatewayRows(db,billingMonth(Date.now()))).find(item=>item.id===request.requestId);
  expect(row).toMatchObject({status:"failed",thb_satang:null});
});

it("blocks the Platform key on kill switch but leaves BYOK available",async()=>{
  const before=await readAiGatewayPolicy(db);
  await setAiPlatformKeyEnabled(db,{enabled:false,actorId:"platform-admin",reason:"Provider incident"});
  expect(await readAiGatewayPolicy(db)).toMatchObject({platformEnabled:false,monthlyRequestLimit:before.monthlyRequestLimit,monthlySpendCapSatang:before.monthlySpendCapSatang});
  await expect(reserveAiGatewayRequest(scope("erpx"),"api.provider.example","model-1")).rejects.toMatchObject({code:"AI_GATEWAY_BLOCKED"});
  const byok=scope("byok");
  await reserveAiGatewayRequest(byok,"api.provider.example","model-1");
  await finishAiGatewayRequest(byok,"succeeded",3,2,{input_tokens:3,output_tokens:2});
  await setAiPlatformKeyEnabled(db,{enabled:true,actorId:"platform-admin",reason:"Incident resolved"});
});

it("reserves the Holding spend allowance before provider I/O",async()=>{
  await updateAiGatewayPolicy(db,{enabled:true,monthlyRequestLimit:1000,monthlySpendCapSatang:5000,reservationSatang:5000,actorId:"platform-admin",reason:"Test spending guard"});
  await expect(reserveAiGatewayRequest(scope("erpx"),"api.provider.example","model-1")).rejects.toMatchObject({code:"AI_GATEWAY_BLOCKED"});
  await updateAiGatewayPolicy(db,{enabled:true,monthlyRequestLimit:1000,monthlySpendCapSatang:10000000,reservationSatang:1000,actorId:"platform-admin",reason:"Restore allowance"});
});

it("requires matching Holding provider statement and exact reconciliation before cost is rateable",async()=>{
  const request=scope("erpx");
  await reserveAiGatewayRequest(request,"api.provider.example","model-1");
  await finishAiGatewayRequest(request,"succeeded",10,5,{input_tokens:10,output_tokens:5});
  const statementId=crypto.randomUUID();
  await recordAiProviderStatement(db,{id:statementId,holdingId,provider:"api.provider.example",month:billingMonth(Date.now()),currency:"USD",amountMinor:20,statementDigest:"a".repeat(64),verifiedBy:"platform-admin"});
  const byok=scope("byok");
  await reserveAiGatewayRequest(byok,"api.provider.example","model-1");
  await finishAiGatewayRequest(byok,"succeeded",2,1,{input_tokens:2,output_tokens:1});
  await expect(recordVerifiedAiCost(db,{requestId:byok.requestId,providerStatementId:statementId,currency:"USD",amountMinor:10,thbSatang:350,fxEvidenceRef:"bank-rate-1",verifiedBy:"platform-admin"})).rejects.toMatchObject({code:"AI_COST_EVIDENCE_CONFLICT"});
  await expect(recordVerifiedAiCost(db,{requestId:request.requestId,providerStatementId:statementId,currency:"USD",amountMinor:10,thbSatang:350,verifiedBy:"platform-admin"})).rejects.toMatchObject({code:"AI_COST_EVIDENCE_INVALID"});
  await recordVerifiedAiCost(db,{requestId:request.requestId,providerStatementId:statementId,currency:"USD",amountMinor:10,thbSatang:350,fxEvidenceRef:"bank-rate-1",verifiedBy:"platform-admin"});
  await expect(reconcileAiProviderStatement(db,statementId)).rejects.toMatchObject({code:"AI_STATEMENT_UNBALANCED"});
  expect((await safeAiGatewayRows(db,billingMonth(Date.now()))).find(row=>row.id===request.requestId)?.reconciled_at).toBeNull();
  const second=scope("erpx");
  await reserveAiGatewayRequest(second,"api.provider.example","model-1");
  await finishAiGatewayRequest(second,"succeeded",2,1,{input_tokens:2,output_tokens:1});
  await recordVerifiedAiCost(db,{requestId:second.requestId,providerStatementId:statementId,currency:"USD",amountMinor:10,thbSatang:350,fxEvidenceRef:"bank-rate-1",verifiedBy:"platform-admin"});
  await reconcileAiProviderStatement(db,statementId);
  expect((await safeAiGatewayRows(db,billingMonth(Date.now()))).filter(row=>[request.requestId,second.requestId].includes(row.id)).every(row=>row.reconciled_at!==null)).toBe(true);
});
