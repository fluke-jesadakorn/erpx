import {unitsToDecimal} from '@/lib/finance-money';
import type {FinanceError} from '../domain/invoice-line-revision';
import type {AssetCarryBasis,AssetCarryBook,AssetCarryingRow as Row} from '../domain/asset-carrying';
export interface AssetCarryingPorts{error:FinanceError;hash(value:string):Promise<string>;lines(ids:string[]):Promise<Row[]>;legacy(assetId:string):Promise<Row[]>;runs(assetId:string):Promise<Row[]>;openingRule(companyId:string,primaryChartId:string,secondaryChartId:string):Promise<Row|null>;primaryOpening(companyId:string,chartId:string,effectiveDate:string):Promise<Row[]>;mappedOpening(ruleId:string,companyId:string):Promise<Row[]>;projections(primaryIds:string[],chartId:string):Promise<Row[]>;adjustments(companyId:string,chartId:string,primaryIds:string[]):Promise<Row[]>;}
export function createAssetCarryingEvidence(ports:AssetCarryingPorts){
const month = (d: string) => Number(d.slice(0, 4)) * 12 + Number(d.slice(5, 7));
function fail(message: string): never {
  throw ports.error(409, "ASSET_HISTORY_RECONCILIATION_REQUIRED", message);
}
async function lines( ids: string[]) {
  if (!ids.length) return [] as Row[];
  return (
    await ports.lines(ids));
}
function balance(rows: Row[], account: string) {
  return rows
    .filter((l) => l.account_id === account)
    .reduce(
      (sum, l) =>
        sum + BigInt(l.base_debit_minor!) - BigInt(l.base_credit_minor!),
      BigInt(0),
    );
}
function owned(rows: Row[], company: string, chart?: string) {
  if (
    rows.some(
      (r) =>
        r.company_id !== company ||
        (chart && r.chart_id !== chart) ||
        !["posted", "reversed"].includes(String(r.status)),
    )
  )
    fail(
      "Asset source journals must remain posted in their original Company and book.",
    );
}

/** Attribute a reviewed book opening to an asset using its retained source journals,
 * never the balance of a shared accumulated-depreciation account. */
async function assetCarryingEvidence(
  
  a: Row,
  books: AssetCarryBook[],
) {
  const legacy = (
    await ports.legacy(String(a.id)));
  if (
    legacy.some(
      (r, i) =>
        r.company_id !== a.company_id ||
        r.ordinal !== i + 1 ||
        month(String(r.entry_date)) !== month(String(a.in_service_date)) + i,
    )
  )
    fail("Reconcile the original monthly asset sequence before adopting it.");
  const runs = (
    await ports.runs(String(a.id)));
  const ids = new Set<string>([
    String(a.activation_journal_id),
    ...legacy.map((r) => String(r.journal_id)),
  ]);
  for (const r of runs) {
    for (const j of JSON.parse(String(r.journal_ids_json)) as Array<{
      id: string;
    }>)
      ids.add(j.id);
    for (const j of JSON.parse(String(r.reversal_ids_json)) as string[])
      ids.add(j);
  }
  const sourceLines = await lines( [...ids]);
  owned(sourceLines, String(a.company_id));
  if ([...ids].some((id) => !sourceLines.some((l) => l.journal_id === id)))
    fail("Retain every asset acquisition, depreciation and reversal journal.");
  const primary = sourceLines.filter((l) => l.chart_id === a.chart_id),
    primaryIds = [...new Set(primary.map((l) => String(l.journal_id)))];
  const acquisition = primary.filter(
    (l) => l.journal_id === a.activation_journal_id,
  );
  if (
    acquisition.some(
      (l) => l.source_type !== "asset_acquisition" || l.source_id !== a.id,
    ) ||
    balance(acquisition, String(a.account_id)) !== BigInt(a.cost_minor!)
  )
    fail("Asset cost differs from its original acquisition journal.");
  for (const r of legacy) {
    const posting = primary.filter((l) => l.journal_id === r.journal_id);
    if (
      !posting.length ||
      posting.some(
        (l) =>
          l.source_type !== "asset_depreciation" ||
          l.source_id !== `${a.id}:${r.depreciation_month}` ||
          l.entry_date !== r.entry_date,
      ) ||
      balance(posting, String(a.depreciation_account_id)) !==
        -BigInt(r.amount_minor!) ||
      balance(posting, String(a.depreciation_expense_account_id)) !==
        BigInt(r.amount_minor!)
    )
      fail(
        "Original asset depreciation differs from its immutable monthly journal.",
      );
  }
  if (
    balance(primary, String(a.account_id)) !== BigInt(a.cost_minor!) ||
    balance(primary, String(a.depreciation_account_id)) !==
      -BigInt(a.accumulated_depreciation_minor!)
  )
    fail("Asset carrying value differs from its owned primary-book journals.");
  const activeRuns = runs.filter((r) => r.status === "posted");
  const ordinal = legacy.length + activeRuns.length;
  if (activeRuns.some((r, i) => r.ordinal !== legacy.length + i + 1))
    fail(
      "Reconcile the reviewed depreciation sequence before changing its basis.",
    );
  const latest = activeRuns.at(-1),
    lastResult = latest
      ? (JSON.parse(String(latest.result_json)) as {
          books: Array<{ chartId: string; units: number }>;
        })
      : null;
  const throughDate = latest
    ? String(latest.entry_date)
    : legacy.at(-1)
      ? String(legacy.at(-1)!.entry_date)
      : String(a.in_service_date);
  const calculated: AssetCarryBasis["books"] = [],
    proof: unknown[] = [legacy, runs, sourceLines];
  for (const b of books) {
    let accumulated: bigint;
    if (!b.ruleId) accumulated = BigInt(a.accumulated_depreciation_minor!);
    else {
      const start = await ports.openingRule(String(a.company_id),String(a.chart_id),b.chartId);
      if (!start)
        fail(
          "Review the secondary book opening before adopting asset carrying values.",
        );
      const opening = (
        await ports.primaryOpening(String(a.company_id),String(a.chart_id),String(start.effective_date)));
      if ((await ports.hash(JSON.stringify(opening))) !== start.opening_hash)
        fail(
          "The primary history no longer agrees with the reviewed book opening.",
        );
      const mappedOpening = (
        await ports.mappedOpening(String(start.id),String(a.company_id)));
      const restored = mappedOpening.map((l) => ({
        accountId: l.primary_account_id,
        dimensions: l.dimension_json,
        minor: l.minor,
      }));
      if (JSON.stringify(restored) !== JSON.stringify(opening))
        fail(
          "The retained secondary opening differs from its independently reviewed amounts.",
        );
      const before = primary.filter(
          (l) => String(l.entry_date) < String(start.effective_date),
        ),
        afterIds = primaryIds.filter((id) =>
          primary.some(
            (l) =>
              l.journal_id === id &&
              String(l.entry_date) >= String(start.effective_date),
          ),
        );
      const projections = afterIds.length
        ? (
            await ports.projections(afterIds,b.chartId))
        : [];
      if (projections.length !== afterIds.length)
        fail("Retain the asset's automatic posting in every enabled book.");
      const projected = await lines(
        projections.map((p) => String(p.secondary_journal_id)),
      );
      owned(projected, String(a.company_id), b.chartId);
      const ownSecondary = sourceLines.filter((l) => l.chart_id === b.chartId);
      const adjustments = primaryIds.length
        ? (
            await ports.adjustments(String(a.company_id),b.chartId,primaryIds))
        : [];
      const adjustmentLines = await lines(
        adjustments.flatMap((r) =>
          [r.journal_id, r.reversal_journal_id].filter(
            (id): id is string => typeof id === "string" && !ids.has(id),
          ),
        ),
      );
      owned(adjustmentLines, String(a.company_id), b.chartId);
      const secondary = [...projected, ...ownSecondary, ...adjustmentLines];
      const secondaryCost =
        balance(before, String(a.account_id)) +
        balance(secondary, b.costAccount);
      if (secondaryCost !== BigInt(a.cost_minor!))
        fail(
          "Adopting a cost revaluation requires its asset owner; depreciation policy cannot silently replace acquisition cost.",
        );
      accumulated = -(
        balance(before, String(a.depreciation_account_id)) +
        balance(secondary, b.accumulatedAccount)
      );
      if (
        accumulated < BigInt(0) ||
        accumulated > BigInt(a.cost_minor!) - BigInt(a.residual_minor!)
      )
        fail(
          "Secondary asset depreciation exceeds the reviewed cost and residual value.",
        );
      proof.push({
        start,
        opening,
        mappedOpening,
        projections,
        projected,
        adjustments,
        adjustmentLines,
      });
    }
    calculated.push({
      chartId: b.chartId,
      accumulated: unitsToDecimal(accumulated),
      units:
        lastResult?.books.find((r) => r.chartId === b.chartId)?.units ??
        lastResult?.books.find((r) => r.chartId === a.chart_id)?.units ??
        0,
    });
  }
  return {
    latestAccountingDate: [
      String(a.in_service_date),
      ...sourceLines.map((l) => String(l.entry_date)),
      ...runs.flatMap((r) => [
        String(r.entry_date),
        ...(r.reversal_entry_date ? [String(r.reversal_entry_date)] : []),
      ]),
    ]
      .sort()
      .at(-1)!,
    basis: {
      ordinal,
      throughDate,
      books: calculated,
    } satisfies AssetCarryBasis,
    proof,
  };
}

return assetCarryingEvidence;}
