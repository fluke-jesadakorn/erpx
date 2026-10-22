import {beforeAll,expect,it,vi} from 'vitest';
import {env,applyMigrations} from "../../support/runtime-env";
import {seedExpenseCompany} from "../../../fixtures/expenses/company";
import {runLegalAiEngineeringFixture} from "../../../fixtures/legal/builders/legal-ai-engineering";
import {legalCommand,detail} from "@erpx/server-legal/public/legal/service";
import {readAiGatewayPolicy} from "@erpx/server-agent/public/ai-gateway";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({runtimeEnv:async()=>env,holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES})}));
beforeAll(async()=>{await applyMigrations(env.H001_DB,env.STAFF_TEST_MIGRATIONS);});
it('executes declared provider boundaries with native in-flight ACL and pre-network budget refusal, preserving manual work and limits',async()=>{
 const f=await seedExpenseCompany(env.H001_DB,env.H001_FILES),manager=await f.member('legal-ai-maker','legal_manager'),reviewer=await f.member('legal-ai-reviewer','legal_counsel');
 await legalCommand(env.H001_DB,manager,{action:'settings',enabled:true,aiEnabled:false},crypto.randomUUID());const before=await readAiGatewayPolicy(env.H001_DB);
 const result=await runLegalAiEngineeringFixture(env.H001_DB,env as unknown as Parameters<typeof runLegalAiEngineeringFixture>[1],manager,reviewer);
 expect(result).toMatchObject({promptSeparated:true,providerCalls:4,budgetProviderCalls:0,gatewayCount:0,manualDocuments:2,automaticSideEffects:0,providerCertification:'pending'});
 expect(result.generated).toMatchObject({prompt_injection:{state:'proposed'},wrong_citation:{state:'failed',error:'LEGAL_AI_CITATION'},access_revoked_in_flight:{state:'failed',error:'LEGAL_NOT_FOUND'},provider_failure:{state:'failed',error:'AI_PROVIDER_ERROR'},budget_refusal:{state:'failed',error:'AI_GATEWAY_BLOCKED'}});
 expect((await detail(env.H001_DB,manager,result.itemId)).documents).toHaveLength(2);await expect(detail(env.H001_DB,reviewer,result.itemId)).rejects.toMatchObject({code:'LEGAL_NOT_FOUND'});
 const after=await readAiGatewayPolicy(env.H001_DB);expect([after.platformEnabled,after.monthlyRequestLimit,after.monthlySpendCapSatang,after.reservationSatang]).toEqual([before.platformEnabled,before.monthlyRequestLimit,before.monthlySpendCapSatang,before.reservationSatang]);
 expect(await env.H001_DB.prepare('SELECT ai_enabled FROM legal_settings WHERE company_id=?').bind(f.company).first('ai_enabled')).toBe(false);
});
