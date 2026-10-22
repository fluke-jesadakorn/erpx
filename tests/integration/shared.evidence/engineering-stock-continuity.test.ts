import {beforeAll,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {saveMaster} from "@erpx/server-commerce/public/commerce";
import {saveCount,transitionCount,countSchema} from "@erpx/server-inventory/public/inventory-counts";
import {assignFlowPrimary,resolveFlow} from "@erpx/server-workflow/public/flow-routing";
import {readStockSnapshot,reconcileStockSnapshots} from "../../../fixtures/inventory/builders/engineering-stock-continuity";
import {documentPostingPreview} from "@erpx/server-finance/public/document-posting-preview";

vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});

it('reads native count movements and posted functional GL from the same holding snapshot',async()=>{
 const f=await expenseFixture(),warehouseId=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Native stock continuity sample'},key())).id),productId=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Independently counted stock',price:'10'},key())).id);
 await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'5400','Observed count variance','expense','THB',?,?)").bind(`stock-proof-variance-${f.company}`,f.chart,f.company,Date.now(),Date.now()).run();
 const before=await readStockSnapshot(db),id=String((await saveCount(f.owner,null,countSchema.parse({warehouseId,countDate:'2026-09-02',reason:'Count exact physical units and reviewed functional value.',lines:[{productId,countedQuantity:'3',unitCost:'4'}]}),key())).id);
 expect(await documentPostingPreview(f.owner,'stock_count',id,db)).toMatchObject({state:'ready',posted:false,debit:1200,credit:1200});
 expect(await documentPostingPreview({...f.owner,companyId:'other'},'stock_count',id,db)).toBeNull();
 await transitionCount(f.owner,id,'submit',{expectedVersion:1,attachmentSetIds:[]},key());
 for(const [version,step,action] of [[2,'submitted','approve'],[3,'approved','post']] as const){const flow=await resolveFlow(db,f.owner,'stock_count',id);await assignFlowPrimary(db,f.owner,'stock_count',id,{stepKey:step,userId:f.manager.userId,expectedVersion:flow.version,recordVersion:version,reason:'Independent inventory Finance reviewer for observed stock proof.',saveDefault:false},key());await transitionCount(f.manager,id,action,{expectedVersion:version,attachmentSetIds:[],reason:'Physical quantity and exact posted valuation independently reviewed.'},key());}
 const after=await readStockSnapshot(db),proof=reconcileStockSnapshots(before,after);
 expect(await documentPostingPreview(f.manager,'stock_count',id,db)).toMatchObject({state:'ready',posted:true,debit:1200,credit:1200});
 expect(proof.passed,JSON.stringify({companies:proof.companies,invalidRows:proof.invalidRows,invalidTrackedBalances:proof.invalidTrackedBalances,unmappedWipMovements:proof.unmappedWipMovements,stockAssertions:proof.stockAssertions,wip:proof.wip})).toBe(true);
 expect(proof.companies.find(row=>row.companyId===f.company)).toMatchObject({physicalMinor:'1200',inTransitMinor:'0',ledgerMinor:'1200',stockMinor:'1200',differenceMinor:'0',stockRows:1});
 expect(proof.stockAssertions.find(row=>row.warehouseId===warehouseId&&row.productId===productId)).toMatchObject({expectedQuantityMilli:'3000',quantityMilli:'3000',expectedValueMinor:'1200',valueMinor:'1200'});
 expect(after.movements.filter(row=>row.companyId===f.company)).toEqual([expect.objectContaining({sourceType:'stock_count',sourceId:id,quantityMilli:'3000',valueMinor:'1200'})]);
 expect(proof.newMovements).toBe(1);
});
