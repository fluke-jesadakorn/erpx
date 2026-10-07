import { allocateUnits, decimalToUnits, multiplyBps, unitsToDecimal } from "@/lib/finance-money";
export type CalculationKind = "allocation" | "cash_flow" | "consolidation" | "thai_statutory" | "manufacturing_cost" | "reconciliation";
type Json = Record<string, unknown>;
const money = (value: unknown, scale: number) => decimalToUnits(String(value ?? "0"), scale);
const text = (value: bigint, scale: number) => unitsToDecimal(value, scale);
export function calculateFinance(kind: CalculationKind, input: Json, scale = 2): Json {
    if (kind === "allocation") {
        const total = money(input.total, scale), weights = (input.weights as Array<{
            key: string;
            weight: string;
        }>).map(row => ({ key: row.key, weight: decimalToUnits(row.weight, 6) }));
        const lines = allocateUnits(total, weights).map(row => ({ key: row.key, amount: text(row.units, scale) }));
        return { total: text(total, scale), lines, controlTotal: text(lines.reduce((sum, row) => sum + money(row.amount, scale), BigInt(0)), scale) };
    }
    if (kind === "cash_flow") {
        const operating = money(input.profit, scale) + money(input.nonCashAdjustments, scale) + money(input.workingCapitalChange, scale);
        const investing = money(input.investing, scale), financing = money(input.financing, scale), fx = money(input.fxEffect, scale);
        const opening = money(input.openingCash, scale), closing = money(input.closingCash, scale), calculated = opening + operating + investing + financing + fx;
        return { openingCash: text(opening, scale), operating: text(operating, scale), investing: text(investing, scale), financing: text(financing, scale), fxEffect: text(fx, scale), netChange: text(operating + investing + financing, scale), calculatedClosing: text(calculated, scale), closingCash: text(closing, scale), unexplainedDifference: text(closing - calculated, scale), balanced: closing === calculated };
    }
    if (kind === "consolidation") {
        const consideration = money(input.consideration, scale), previous = money(input.previousInterest, scale), netAssets = money(input.fairValueNetAssets, scale), nciFair = money(input.nciFairValue, scale);
        const acquisitionDifference = consideration + previous + nciFair - netAssets;
        const profit = money(input.subsidiaryProfit, scale), oci = money(input.subsidiaryOci, scale), dividends = money(input.subsidiaryDividends, scale), parentBps = Number(input.parentBps);
        const nciBps = 10000 - parentBps, nciClosing = money(input.openingNci, scale) + multiplyBps(profit, nciBps) + multiplyBps(oci, nciBps) - multiplyBps(dividends, nciBps) + money(input.nciOwnershipChange, scale);
        const associateBeforeLoss = money(input.associateOpening, scale) + multiplyBps(money(input.associateProfit, scale), Number(input.associateBps)) + multiplyBps(money(input.associateOci, scale), Number(input.associateBps)) - money(input.associateDividends, scale) - money(input.associateImpairment, scale);
        const associateClosing = associateBeforeLoss < BigInt(0) ? BigInt(0) : associateBeforeLoss;
        return { goodwill: text(acquisitionDifference > BigInt(0) ? acquisitionDifference : BigInt(0), scale), bargainPurchase: text(acquisitionDifference < BigInt(0) ? -acquisitionDifference : BigInt(0), scale), nciClosing: text(nciClosing, scale), nciProfitShare: text(multiplyBps(profit, nciBps), scale), equityMethodClosing: text(associateClosing, scale), unrecognizedAssociateLoss: text(associateBeforeLoss < BigInt(0) ? -associateBeforeLoss : BigInt(0), scale) };
    }
    if (kind === "thai_statutory") {
        const outputVat = money(input.outputVat, scale), inputVat = money(input.inputVat, scale), disallowed = money(input.disallowedInputVat, scale), prior = money(input.priorCredit, scale), vatPayable = outputVat - (inputVat - disallowed) - prior;
        const wht3 = money(input.pnd3Withholding, scale), wht53 = money(input.pnd53Withholding, scale), vat36 = money(input.pp36Vat, scale), wht54 = money(input.pnd54Withholding, scale);
        return { pp30: { outputVat: text(outputVat, scale), allowableInputVat: text(inputVat - disallowed, scale), carriedCredit: text(prior, scale), payable: text(vatPayable > BigInt(0) ? vatPayable : BigInt(0), scale), creditForward: text(vatPayable < BigInt(0) ? -vatPayable : BigInt(0), scale) }, withholding: { pnd3: text(wht3, scale), pnd53: text(wht53, scale), pnd54: text(wht54, scale) }, pp36: text(vat36, scale), totalDue: text((vatPayable > BigInt(0) ? vatPayable : BigInt(0)) + wht3 + wht53 + wht54 + vat36, scale) };
    }
    if (kind === "manufacturing_cost") {
        let quantity = BigInt(String(input.openingQuantityMilli ?? 0)), value = money(input.openingValue, scale);
        for (const receipt of (input.receipts ?? []) as Array<{
            quantityMilli: string;
            value: string;
        }>) {
            quantity += BigInt(receipt.quantityMilli);
            value += money(receipt.value, scale);
        }
        let issued = BigInt(0);
        for (const issue of (input.issues ?? []) as Array<{
            quantityMilli: string;
        }>) {
            const qty = BigInt(issue.quantityMilli);
            if (qty > quantity)
                throw new Error("Material issue exceeds available stock.");
            const cost = quantity === BigInt(0) ? BigInt(0) : (value * qty) / quantity;
            quantity -= qty;
            value -= cost;
            issued += cost;
        }
        const labor = money(input.labor, scale), overhead = money(input.overhead, scale), abnormal = money(input.abnormalScrapCost, scale), capitalized = issued + labor + overhead - abnormal, output = BigInt(String(input.outputQuantityMilli ?? 0));
        if (output <= BigInt(0) && capitalized !== BigInt(0))
            throw new Error("A costed order requires positive output.");
        return { closingMaterialQuantityMilli: String(quantity), closingMaterialValue: text(value, scale), issuedMaterialCost: text(issued, scale), wipCost: text(capitalized, scale), abnormalScrapExpense: text(abnormal, scale), outputQuantityMilli: String(output), unitCostPerThousand: text(output ? capitalized * BigInt(1000) / output : BigInt(0), scale) };
    }
    const bank = money(input.bankAmount, scale), allocations = (input.allocations ?? []) as Array<{
        reference: string;
        amount: string;
    }>, allocated = allocations.reduce((sum, row) => sum + money(row.amount, scale), BigInt(0));
    return { bankAmount: text(bank, scale), allocated: text(allocated, scale), difference: text(bank - allocated, scale), balanced: bank === allocated, allocations };
}
