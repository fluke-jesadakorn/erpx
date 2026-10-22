import type {z} from 'zod';
import {decimalToMinor,minorToDecimal} from "@erpx/feature-finance/lib/finance-money";
import {financeCsv} from "@erpx/feature-finance/lib/finance-csv";
import type {FinanceError} from '@erpx/server-finance/domain/invoice-line-revision';
import type {financialReportSchema,LedgerLine,RetainedReport} from '@erpx/server-finance/domain/planning';
export const canonicalDimensions = (d: Record<string, string>) =>
  JSON.stringify(
    Object.fromEntries(
      Object.entries(d).sort(([a], [b]) => a.localeCompare(b)),
    ),
  );

export function createFinancePlanningCalculations(error:FinanceError){
function money(s: string) {
  try {
    return decimalToMinor(s);
  } catch {
    throw error(
      422,
      "INVALID_MONEY",
      "Enter an exact two-decimal money amount.",
    );
  }
}

function safe(n: bigint) {
  if (
    n > BigInt(Number.MAX_SAFE_INTEGER) ||
    n < BigInt(Number.MIN_SAFE_INTEGER)
  )
    throw error(
      422,
      "MONEY_OVERFLOW",
      "Amount exceeds supported precision.",
    );
  return Number(n);
}

function nextMonth(start: string, offset: number) {
  const [year, month] = start.split("-").map(Number),
    date = new Date(Date.UTC(year, month - 1 + offset, 1));
  return date.toISOString().slice(0, 7);
}

function reportValues(
  rows: z.infer<typeof financialReportSchema>["rows"],
  source: LedgerLine[],
  start: string,
) {
  const result: {
      key: string;
      label: string;
      value: string | null;
      minor: number | null;
      unit: string;
      journalIds: string[];
    }[] = [],
    values = new Map<string, bigint | null>();
  for (const r of rows) {
    let n: bigint | null = BigInt(0),
      ids: string[] = [];
    if (r.kind === "accounts") {
      const selected = source.filter(
        (l) =>
          r.accountIds.includes(l.accountId) &&
          (r.basis === "cumulative" ||
            (l.entryDate >= start && l.sourceType !== "fiscal_close" && Number(l.isFiscalClose) !== 1)) &&
          Object.entries(r.dimensions).every(
            ([k, v]) => JSON.parse(l.dimensions)[k] === v,
          ),
      );
      n =
        selected.reduce(
          (s, l) => s + BigInt(l.debit) - BigInt(l.credit),
          BigInt(0),
        ) * (r.sign === "debit" ? BigInt(1) : BigInt(-1));
      ids = [...new Set(selected.map((l) => l.journalId))];
    } else {
      const v = r.references.map((k) => values.get(k) ?? null);
      if (v.some((x) => x === null)) n = null;
      else if (r.kind === "sum")
        n = (v as bigint[]).reduce((s, a) => s + a, BigInt(0));
      else if (r.kind === "difference") n = v[0]! - v[1]!;
      else if(v[1]===BigInt(0))n=null;
      else {const numerator=v[0]!*BigInt(10000),denominator=v[1]!,negative=(numerator<BigInt(0))!==(denominator<BigInt(0)),absolute=numerator<BigInt(0)?-numerator:numerator,divisor=denominator<BigInt(0)?-denominator:denominator;n=((absolute+divisor/BigInt(2))/divisor)*(negative?BigInt(-1):BigInt(1));}
      ids = [
        ...new Set(
          result
            .filter((x) => r.references.includes(x.key))
            .flatMap((x) => x.journalIds),
        ),
      ];
    }
    values.set(r.key, n);
    result.push({
      key: r.key,
      label: r.label,
      value: n === null ? null : minorToDecimal(safe(n)),
      minor: n === null ? null : safe(n),
      unit: r.kind === "ratio" ? "percent" : "money",
      journalIds: ids,
    });
  }
  return result;
}

function calculateCashFlow(
  source: LedgerLine[],
  start: string,
  policy: NonNullable<z.infer<typeof financialReportSchema>["cashFlow"]>,
) {
  const bank = new Set(policy.bankAccountIds),
    groups = new Map(policy.classifications.map((r) => [r.accountId, r.kind])),
    totals = {
      operating: BigInt(0),
      investing: BigInt(0),
      financing: BigInt(0),
    },
    trace: { journalId: string; kind: string; minor: number }[] = [];
  const opening = source
      .filter(
        (l) =>
          bank.has(l.accountId) &&
          (l.entryDate < start || l.sourceType === "book_opening"),
      )
      .reduce((s, l) => s + BigInt(l.debit) - BigInt(l.credit), BigInt(0)),
    closing = source
      .filter((l) => bank.has(l.accountId))
      .reduce((s, l) => s + BigInt(l.debit) - BigInt(l.credit), BigInt(0));
  const journalIds = new Set(
    source
      .filter(
        (l) =>
          bank.has(l.accountId) &&
          l.entryDate >= start &&
          l.sourceType !== "book_opening",
      )
      .map((l) => l.journalId),
  );
  for (const jid of journalIds) {
    const lines = source.filter((l) => l.journalId === jid),
      net = lines
        .filter((l) => bank.has(l.accountId))
        .reduce((s, l) => s + BigInt(l.debit) - BigInt(l.credit), BigInt(0));
    if (net === BigInt(0)) continue;
    for (const l of lines.filter((l) => !bank.has(l.accountId))) {
      const kind = groups.get(l.accountId);
      if (!kind)
        throw error(
          422,
          "CASH_FLOW_MAPPING_REQUIRED",
          `Classify the noncash account ${l.accountId} before creating the cash-flow snapshot.`,
        );
      const value = BigInt(l.credit) - BigInt(l.debit);
      totals[kind] += value;
      trace.push({ journalId: jid, kind, minor: safe(value) });
    }
  }
  const change = totals.operating + totals.investing + totals.financing;
  if (opening + change !== closing)
    throw error(
      409,
      "CASH_FLOW_RECONCILIATION",
      "Cash-flow movements do not reconcile to bank balances.",
    );
  return {
    opening: minorToDecimal(safe(opening)),
    operating: minorToDecimal(safe(totals.operating)),
    investing: minorToDecimal(safe(totals.investing)),
    financing: minorToDecimal(safe(totals.financing)),
    change: minorToDecimal(safe(change)),
    closing: minorToDecimal(safe(closing)),
    trace,
  };
}

function reportCsv(result:RetainedReport,escapeFormulas=true){return financeCsv(["Row","Amount","Unit"],[...result.rows.map(r=>[r.label,r.value,r.unit]),...(result.cashFlow?["opening","operating","investing","financing","change","closing"].map(key=>[`Cash flow: ${key}`,result.cashFlow![key],"money"]):[])],escapeFormulas);}
return {money,safe,nextMonth,reportValues,calculateCashFlow,reportCsv};}
