/** Development oracle only: expectations come from the retained TypeScript implementation. */
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { depreciationAmount, halfAway, instrumentSchedule, monthlyDate } from "../../../packages/server-finance/src/domain/finance-instrument-policy";
import { decimalToUnits, unitsToDecimal } from "../../../packages/feature-finance/src/lib/finance-money";
import { calculateFinance } from "../../../packages/server-finance/src/domain/finance-calculations";
import { calculateInvoiceLines } from "../../../packages/server-finance/src/domain/finance-invoice-lines";
import { createJournalPreparation } from "../../../packages/server-finance/src/application/journal";
import { financeNodeOracle } from "./finance-node-oracle";

const sourcePaths = ["packages/server-finance/src/domain/finance-instrument-policy.ts", "packages/feature-finance/src/lib/finance-money.ts", "packages/server-finance/src/domain/finance-calculations.ts", "packages/server-finance/src/domain/finance-invoice-lines.ts", "packages/server-finance/src/application/journal.ts", "packages/server-finance/src/domain/finance-policy.ts", "packages/feature-finance/src/lib/company-money.ts", "packages/server-finance/src/application/functional-fx.ts", "packages/catalogue/src/iso-4217-current.json"];
const sourceCommit = "a5ed8737a2f05eff1ee9841fda4a094b3926d732";
const sources = await Promise.all(sourcePaths.map(async (path) => ({ path, sha256: createHash("sha256").update(await readFile(path)).digest("hex") })));
for (const source of sources) {
  const baseline = execFileSync("git", ["show", `${sourceCommit}:${source.path}`]);
  if (createHash("sha256").update(baseline).digest("hex") !== source.sha256) throw new Error(`Oracle source changed: ${source.path}`);
}
function outcome(run: () => unknown) {
  try { return { value: run() }; }
  catch (error) { return { error: error instanceof Error ? error.message : String(error), validation: error instanceof Error && error.name === "ZodError" }; }
}
const base = { kind: "loan", number: "L-1", name: "Reviewed loan", chartId: "chart", currency: "thb", startDate: "2024-01-31", months: 12, annualRateBps: 500, payment: "100.00", principal: "1000.00", bankAccountId: "bank", liabilityAccountId: "liability", interestAccountId: "interest", reason: "Approved terms" };
const inputs = [
  base, { ...base, annualRateBps: 0 }, { ...base, months: 1 },
  { ...base, firstDueDate: "2024-02-29" },
  ...[0, 1, 500, 100000].map((annualRateBps) => ({ ...base, kind: "lease", principal: undefined, annualRateBps, rightOfUseAccountId: "rou", accumulatedAccountId: "accum", depreciationAccountId: "expense" })),
  { ...base, months: 240, principal: "20000.00" },
  { ...base, months: 1, principal: "90071992547409.91" },
  { ...base, months: 1, principal: "90071992547409.92" },
  { ...base, principal: "0.01" }, { ...base, annualRateBps: 100000 },
  { ...base, months: 0 }, { ...base, months: 241 }, { ...base, payment: "0" },
  { ...base, firstDueDate: "2024-01-31" }, { ...base, startDate: "2023-02-29" },
  { ...base, principal: undefined }, { ...base, kind: "lease" },
  { ...base, number: "\ufeff L-2 \u00a0", name: "  Name  ", reason: "  Approved  " },
];
const schedules = inputs.map((input, index) => ({ id: `schedule-${index}`, input, expected: outcome(() => instrumentSchedule(input as Parameters<typeof instrumentSchedule>[0])) }));
const wireNumbers = [
  ...["12.0", "12e0", "12.5", "12e100", "-0.0", "\"12\"", "true", "null"].map(value => JSON.stringify(base).replace('"months":12', `"months":${value}`)),
  ...["500.0", "500e0", "-0.0", "100001.0"].map(value => JSON.stringify(base).replace('"annualRateBps":500', `"annualRateBps":${value}`)),
].map(raw => ({raw, expected: outcome(() => instrumentSchedule(JSON.parse(raw)))}));
const depBase = { cost: "100001", residual: "1000", life: 12, ordinal: 1, accumulated: "0", policy: { method: "straight_line" } };
const depreciationInputs = [
  depBase, { ...depBase, ordinal: 12, accumulated: "90000" },
  { ...depBase, ordinal: 13 }, { ...depBase, ordinal: 13, accumulated: "99001" },
  { ...depBase, ordinal: 5, accumulated: "33000", basisOrdinal: 4, basisAccumulated: "33000" },
  { ...depBase, accumulated: "50000" }, { ...depBase, residual: "200000" },
  ...[1, 2500, 10000].map((annualRateBps) => ({ ...depBase, policy: { method: "declining_balance", annualRateBps } })),
  ...[0, 1, 99, 100].map((units) => ({ ...depBase, policy: { method: "units_of_production", totalUnits: 100 }, priorUnits: 0, units })),
  { ...depBase, policy: { method: "units_of_production", totalUnits: 100 }, priorUnits: 50, units: 10, accumulated: "40000", basisUnits: 50, basisAccumulated: "40000" },
  { ...depBase, policy: { method: "units_of_production", totalUnits: 100 }, priorUnits: 99, units: 2 },
  { ...depBase, policy: { method: "declining_balance", annualRateBps: 0 } },
];
const depreciation = depreciationInputs.map((input, index) => ({ id: `depreciation-${index}`, input, expected: outcome(() => depreciationAmount({ ...input, cost: BigInt(input.cost), residual: BigInt(input.residual), accumulated: BigInt(input.accumulated), ...("basisAccumulated" in input ? { basisAccumulated: BigInt(input.basisAccumulated as string) } : {}) } as Parameters<typeof depreciationAmount>[0]).toString()) }));
const money = ["0", "-0.00", "1.2300", "-1.005", "999999999999999999999999999.99", "\ufeff 12.50 \u00a0", "\u008512.50", "1.", "+1", "1.2.3"].flatMap((input) => [0, 2, 12].map((scale) => ({ input, scale, expected: outcome(() => { const units = decimalToUnits(input, scale); return { units: units.toString(), decimal: unitsToDecimal(units, scale) }; }) })));
const rounding = [-151, -150, -149, -50, -49, 0, 49, 50, 149, 150, 151].map((numerator) => ({ numerator: String(numerator), denominator: "100", value: halfAway(BigInt(numerator), 100n).toString() }));
const dates = ["2024-01-31", "2023-01-31", "2024-02-29", "0099-12-31", "0100-01-31"].flatMap((anchor) => [-1, 0, 1, 2, 12, 240].map((offset) => ({ anchor, offset, value: monthlyDate(anchor, offset) })));
const flow = { openingCash: "100.00", closingCash: "139.00", profit: "50.00", nonCashAdjustments: "5.00", workingCapitalChange: "-10.00", investing: "-20.00", financing: "15.00", fxEffect: "-1.00" };
const manufacturing = { openingQuantityMilli: "3000", openingValue: "10.00", receipts: [{ quantityMilli: "7000", value: "30.00" }], issues: [{ quantityMilli: "3333" }, { quantityMilli: "6667" }], labor: "5.00", overhead: "2.00", abnormalScrapCost: "1.00", outputQuantityMilli: "1000" };
const calculations = [
  ...[0, 2, 12].flatMap((scale) => [flow, { ...flow, closingCash: "0", openingCash: "999999999999999999999999999999.00" }].map((input) => ({ kind: "cash_flow" as const, scale, input }))),
  ...[manufacturing, { ...manufacturing, issues: [{ quantityMilli: "10001" }] }, { ...manufacturing, outputQuantityMilli: "0" }, { ...manufacturing, openingQuantityMilli: "0", openingValue: "0", receipts: [], issues: [], labor: "0", overhead: "0", abnormalScrapCost: "0", outputQuantityMilli: "0" }].map((input) => ({ kind: "manufacturing_cost" as const, scale: 2, input })),
  ...["0.01", "-0.01", "100000000000000000000.00"].map(total => ({kind: "allocation" as const, scale: 2, input: {total, weights: [{key: "a", weight: "1"}, {key: "b", weight: "2"}, {key: "c", weight: "3"}]}})),
  ...["1000", "-1000"].map(consideration => ({kind: "consolidation" as const, scale: 2, input: {consideration, previousInterest: "100", fairValueNetAssets: "500", nciFairValue: "200", parentBps: 7500, openingNci: "100", subsidiaryProfit: "-0.02", subsidiaryOci: "0.02", subsidiaryDividends: "1", nciOwnershipChange: "2", associateOpening: "10", associateBps: 2000, associateProfit: "-100", associateOci: "0.01", associateDividends: "0", associateImpairment: "1"}})),
  ...["10", "1000"].map(inputVat => ({kind: "thai_statutory" as const, scale: 2, input: {outputVat: "100", inputVat, disallowedInputVat: "5", priorCredit: "1", pnd3Withholding: "3", pnd53Withholding: "4", pp36Vat: "5", pnd54Withholding: "6"}})),
  ...["3", "4"].map(bankAmount => ({kind: "reconciliation" as const, scale: 2, input: {bankAmount, allocations: [{reference: "invoice-1", amount: "1"}, {reference: "invoice-2", amount: "2"}]}})),
].map((row) => ({ ...row, expected: outcome(() => calculateFinance(row.kind, row.input, row.scale)) }));
const invoiceLine = { description: "Reviewed supply", quantity: "1.000", unitPrice: "100.00", taxBps: 700, accountId: "sales", taxAccountId: "tax" };
const invoices = [
  [], [invoiceLine], [invoiceLine, { ...invoiceLine, quantity: "0.001", unitPrice: "5.00" }],
  ...["0.004", "0.005", "0.006", "12345.678", "0", "9007199254740.992"].map((quantity) => [{ ...invoiceLine, quantity }]),
  [{ ...invoiceLine, quantity: "1", unitPrice: "90071992547409.91", taxBps: 0 }],
  [{ ...invoiceLine, quantity: "1", unitPrice: "90071992547409.92", taxBps: 0 }],
  [{ ...invoiceLine, quantity: "1", unitPrice: "90071992547409.91", taxBps: 1 }],
  [invoiceLine, { ...invoiceLine, quantity: "1", unitPrice: "90071992547409.91", taxBps: 0 }],
].map((input) => ({ input, expected: outcome(() => calculateInvoiceLines(input)) }));
const journalLines = [{ accountId: "cash", debit: "0.03", credit: "0" }, { accountId: "sales", debit: "0", credit: "0.01" }, { accountId: "other", debit: "0", credit: "0.02" }];
const journalInputs = [
  ...["1", "0.5", "0.333333", "35.123456", "1000000"].map((fxRate) => ({ lines: journalLines, fxRate })),
  { lines: [{ accountId: "cash", debit: "0.01", credit: "0" }, { accountId: "cash", debit: "0.01", credit: "0" }, { accountId: "sales", debit: "0", credit: "0.02" }], fxRate: "0.5" },
  { lines: [{ accountId: "cash", debit: "1", credit: "1" }], fxRate: "1" },
  { lines: [{ accountId: "cash", debit: "1", credit: "0" }], fxRate: "1" },
  { lines: [{ accountId: "cash", debit: "-1", credit: "0" }], fxRate: "1" },
  { lines: [], fxRate: "1" },
  { lines: journalLines, fxRate: "0" },
  { lines: journalLines, fxRate: "1000000.000001" },
];
const prepareJournal = createJournalPreparation({
  error: (_status, code, message) => Object.assign(new Error(message), { code }),
  now: () => 0, chart: async () => ({ id: "chart", base_currency: "USD" }),
  period: async () => ({}), moneyScale: async () => 2,
  functionalFx: async () => ({ micros: 1_000_000, sourceRateId: null, targetRateId: null, usedLatestFallback: false }),
  accounts: async (_company, _chart, ids) => ids.map((id) => ({ id })), openingExists: async () => false,
});
const journals = await Promise.all(journalInputs.map(async (input) => {
  try {
    const plan = await prepareJournal({ companyId: "company" } as Parameters<typeof prepareJournal>[0], { ...input, chartId: "chart", periodId: "period", entryDate: "2026-01-01", currency: "THB", memo: "", sourceType: "manual", isOpening: false }, "key");
    return { input, expected: { value: { lines: plan.values, total: plan.values.reduce((sum, line) => sum + BigInt(line.debit), 0n).toString(), accountIds: plan.accountIds } } };
  } catch (error) {
    return { input, expected: { error: error instanceof Error ? error.message : String(error), code: (error as { code?: string }).code } };
  }
}));
await mkdir("fixtures/finance/rust-instruments", { recursive: true });
const companyMoney = await financeNodeOracle();
await writeFile("fixtures/finance/rust-instruments/conformance.v1.json", `${JSON.stringify({ version: 1, oracle: "retained TypeScript implementation; development only", sourceCommit, sources, schedules, wireNumbers, depreciation, money, rounding, dates, calculations, invoices, journals, companyMoney }, null, 2)}\n`);
