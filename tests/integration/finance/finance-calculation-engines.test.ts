import { describe, expect, it } from "vitest";
import { allocateUnits, decimalToUnits, multiplyBps, unitsToDecimal } from "../../../src/lib/finance/finance-money";
import { calculateFinance } from "../../../src/server/modules/finance/public/rules/finance-calculations";
import { beforeAll, beforeEach, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { createCalculationRun, decideCalculationRun } from "../../../src/server/modules/finance/public/finance-engines";
vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));

describe("exact finance calculation engine",()=>{
  it("supports currencies with zero, two and three decimal places",()=>{
    expect(unitsToDecimal(decimalToUnits("123",0),0)).toBe("123");
    expect(unitsToDecimal(decimalToUnits("123.45",2),2)).toBe("123.45");
    expect(unitsToDecimal(decimalToUnits("123.456",3),3)).toBe("123.456");
    expect(()=>decimalToUnits("1.001",2)).toThrow();
  });
  it("allocates every minor unit deterministically",()=>{
    expect(allocateUnits(100n,[{key:"b",weight:1n},{key:"a",weight:1n},{key:"c",weight:1n}])).toEqual([{key:"b",units:33n},{key:"a",units:34n},{key:"c",units:33n}]);
    expect(allocateUnits(-5n,[{key:"a",weight:1n},{key:"b",weight:1n}]).reduce((sum,row)=>sum+row.units,0n)).toBe(-5n);
  });
  it("rounds basis-point ownership half away from zero",()=>{
    expect(multiplyBps(101n,5000)).toBe(51n);
    expect(multiplyBps(-101n,5000)).toBe(-51n);
  });
  it("reconciles an indirect cash-flow statement",()=>{
    expect(calculateFinance("cash_flow",{openingCash:"100.00",closingCash:"127.00",profit:"20.00",nonCashAdjustments:"5.00",workingCapitalChange:"-3.00",investing:"-10.00",financing:"12.00",fxEffect:"3.00"})).toMatchObject({operating:"22.00",calculatedClosing:"127.00",unexplainedDifference:"0.00",balanced:true});
  });
  it("calculates goodwill, NCI and equity-method loss floor",()=>{
    expect(calculateFinance("consolidation",{consideration:"800.00",previousInterest:"0",fairValueNetAssets:"900.00",nciFairValue:"180.00",parentBps:8000,openingNci:"180.00",subsidiaryProfit:"100.00",subsidiaryOci:"10.00",subsidiaryDividends:"20.00",nciOwnershipChange:"0",associateOpening:"10.00",associateBps:3000,associateProfit:"-100.00",associateOci:"0",associateDividends:"0",associateImpairment:"0"})).toMatchObject({goodwill:"80.00",nciClosing:"198.00",equityMethodClosing:"0.00",unrecognizedAssociateLoss:"20.00"});
  });
  it("prepares Thai VAT and withholding totals without marking them filed",()=>{
    expect(calculateFinance("thai_statutory",{outputVat:"70",inputVat:"35",disallowedInputVat:"5",priorCredit:"10",pnd3Withholding:"3",pnd53Withholding:"7",pp36Vat:"2",pnd54Withholding:"1"})).toMatchObject({pp30:{payable:"30.00"},totalDue:"43.00"});
  });
  it("uses moving-average value and rejects over-issues and zero-output cost",()=>{
    expect(calculateFinance("manufacturing_cost",{openingQuantityMilli:"1000",openingValue:"100",receipts:[{quantityMilli:"1000",value:"300"}],issues:[{quantityMilli:"500"}],labor:"20",overhead:"10",abnormalScrapCost:"5",outputQuantityMilli:"1000"})).toMatchObject({closingMaterialValue:"300.00",issuedMaterialCost:"100.00",wipCost:"125.00"});
    expect(()=>calculateFinance("manufacturing_cost",{openingQuantityMilli:"1",openingValue:"1",receipts:[],issues:[{quantityMilli:"2"}],labor:"0",overhead:"0",abnormalScrapCost:"0",outputQuantityMilli:"1"})).toThrow("exceeds available stock");
  });
  it("requires all reconciliation allocations to equal the bank amount",()=>{
    expect(calculateFinance("reconciliation",{bankAmount:"100",allocations:[{reference:"A",amount:"40"},{reference:"B",amount:"60"}]})).toMatchObject({difference:"0.00",balanced:true});
  });
});

describe("calculation run controls",()=>{
  let fixture:Awaited<ReturnType<typeof expenseFixture>>;
  beforeAll(async()=>{await applyMigrations(expenseDb,env.STAFF_TEST_MIGRATIONS);});
  beforeEach(async()=>{fixture=await expenseFixture();});
  it("is idempotent and requires an independent reviewer",async()=>{
    const command=key(),input={kind:"cash_flow" as const,currency:"THB",currencyScale:2,periodId:fixture.period,policyVersion:1,input:{openingCash:"100",closingCash:"110",profit:"10",nonCashAdjustments:"0",workingCapitalChange:"0",investing:"0",financing:"0",fxEffect:"0"}},run=await createCalculationRun(fixture.owner,input,command);
    expect(await createCalculationRun(fixture.owner,input,command)).toEqual(run);
    await expect(decideCalculationRun(fixture.owner,String(run.id),{expectedVersion:1,reason:"Reviewed source balances"},"review",key())).rejects.toMatchObject({code:"SELF_REVIEW_FORBIDDEN"});
    expect(await decideCalculationRun(fixture.manager,String(run.id),{expectedVersion:1,reason:"Reviewed source balances"},"review",key())).toMatchObject({status:"reviewed",version:2});
  });
});
