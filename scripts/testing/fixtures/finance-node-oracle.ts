import { execFileSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

/** Run the actual retained algorithms on the pinned Node/ICU runtime, not Bun's locale defaults. */
export async function financeNodeOracle() {
  const node = process.env.ERPX_NODE_BIN ?? "node";
  if (execFileSync(node, ["--version"], { encoding: "utf8" }).trim() !== "v26.11.1") throw new Error("PINNED_FINANCE_ORACLE_NODE_REQUIRED");
  const directory = resolve(".scratch/dev/finance-rust-oracle");
  await mkdir(directory, { recursive: true, mode: 0o700 });
  const entry = resolve(directory, "entry.ts");
  await writeFile(entry, `
import { resolveRate, convertUnits, currencyScale } from ${JSON.stringify(resolve("packages/feature-finance/src/lib/company-money.ts"))};
import { allocateUnits } from ${JSON.stringify(resolve("packages/feature-finance/src/lib/finance-money.ts"))};
import { createFunctionalFx } from ${JSON.stringify(resolve("packages/server-finance/src/application/functional-fx.ts"))};
import { readFileSync } from "node:fs";
const input = JSON.parse(readFileSync(0, "utf8"));
const outcome = (run) => { try { return { value: run() }; } catch (error) { return { error: error.message }; } };
process.stdout.write(JSON.stringify({
  runtime: { node: process.version, icu: process.versions.icu, locale: Intl.Collator().resolvedOptions().locale },
  collation: input.keys.flatMap(left => input.keys.map(right => ({left, right, order: Math.sign(left.localeCompare(right))}))),
  allocations: input.allocations.map(row => ({ ...row, expected: outcome(() => allocateUnits(BigInt(row.total), row.weights.map(weight => ({key: weight.key, weight: BigInt(weight.weight)}))).map(result => ({key: result.key, units: result.units.toString()}))) })),
  rates: input.rates.map(row => ({ ...row, expected: { value: resolveRate(row.rates, row.currency, row.date) } })),
  conversions: input.conversions.map(row => ({ ...row, expected: outcome(() => convertUnits(BigInt(row.amount), row.sourceScale, row.targetScale, row.sourceQuote, row.targetQuote).toString()) })),
  currencies: input.currencies.map(row => ({ ...row, expected: outcome(() => currencyScale(row.currency, row.companyScale)) })),
  functional: await Promise.all(input.functional.map(async row => {
    try {
      const calculate = createFunctionalFx({ rates: async () => row.rates, error: (_status, code, message) => Object.assign(new Error(message), {code}) });
      return {...row, expected: {value: await calculate({companyId: "company"}, row.currency, row.base, row.date)}};
    } catch (error) { return {...row, expected: {error: error.message, code: error.code}}; }
  })),
}));
`, { mode: 0o600 });
  const build = await Bun.build({ entrypoints: [entry], outdir: directory, target: "node", format: "esm", naming: "oracle.mjs", minify: false });
  if (!build.success) throw new Error(`FINANCE_NODE_ORACLE_BUILD_FAILED:${build.logs.map(log => log.message).join(";")}`);
  const keys = ["a", "A", "b", "á", "ä", "é", "e\u0301", "a-1", "a_1", "a.1", "a:1", "a2", "a10", "1", "10", "2", "ฝ่ายขาย", "ฝ่ายบัญชี", "เก่า", "ข่าว", "東京", "😀", "", " "];
  const weights = keys.filter(Boolean).map(key => ({ key, weight: "1" }));
  const allocations = [
    ...["0", "1", "-1", "13", "-13", "900719925474099199999999999"].map(total => ({total, weights})),
    { total: "5", weights: [{key: "x", weight: "2"}, {key: "x", weight: "1"}, {key: "y", weight: "0"}] },
    { total: "5", weights: [] }, { total: "5", weights: [{key: "x", weight: "0"}] },
    { total: "5", weights: [{key: "x", weight: "-1"}] },
  ];
  const baseRate = { id: "rate", currency: "THB", effectiveDate: "2026-01-01", currencyPerUsd: "35", confirmedAt: 100 };
  const records = keys.filter(Boolean).map((id, index) => ({ ...baseRate, id, currencyPerUsd: String(index + 1) }));
  const rates = [
    { rates: records, currency: "THB", date: "2026-01-01" },
    { rates: records, currency: "THB", date: "2025-01-01" },
    { rates: [], currency: "USD", date: "2026-01-01" }, { rates: [], currency: "THB", date: "2026-01-01" },
    { rates: [{ ...baseRate, id: "old" }, { ...baseRate, id: "new", effectiveDate: "2027-01-01", confirmedAt: 200 }], currency: "THB", date: "2026-02-01" },
    { rates: [{ ...baseRate, id: "old" }, { ...baseRate, id: "new", effectiveDate: "2027-01-01", confirmedAt: 200 }], currency: "THB", date: "2025-02-01" },
  ];
  const conversions = ["0", "1", "-1", "150", "-150", "999999999999999999999999999"].flatMap(amount => [
    { amount, sourceScale: 2, targetScale: 2, sourceQuote: "35.123456789012345678", targetQuote: "1" },
    { amount, sourceScale: 0, targetScale: 3, sourceQuote: "1", targetQuote: "35.123456789012345678" },
    { amount, sourceScale: 12, targetScale: 0, sourceQuote: "2", targetQuote: "1" },
  ]);
  for (const sourceQuote of ["0", "-1", "1.0000000000000000001", "x"]) conversions.push({ amount: "1", sourceScale: 2, targetScale: 2, sourceQuote, targetQuote: "1" });
  const currencies = ["THB", "thb", "USD", "JPY", "BHD", "XAU", "XXX", "ZZZ"].flatMap(currency => [null, 0, 2, 12, 13, -1].map(companyScale => ({currency, companyScale})));
  const functional = [
    ...rates.map(row => ({...row, base: "USD"})),
    {rates: records, currency: "USD", base: "THB", date: "2026-01-01"},
    {rates: [], currency: "USD", base: "USD", date: "2026-01-01"},
    {rates: [{...baseRate, currencyPerUsd: "0.000000000000000001"}], currency: "THB", base: "USD", date: "2026-01-01"},
  ];
  const result = JSON.parse(execFileSync(node, [resolve(directory, "oracle.mjs")], { input: JSON.stringify({ keys, allocations, rates, conversions, currencies, functional }), encoding: "utf8" }));
  if (result.runtime.locale !== "en-US" || result.runtime.icu !== "78.3") throw new Error("FINANCE_ORACLE_COLLATION_IDENTITY_CHANGED");
  return result;
}
