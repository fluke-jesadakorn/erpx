import { fiscalCloseSourceSql, capturedFiscalClose } from "./finance-fiscal-close-source";
import "server-only";
import { z } from "zod";
import { decimalToUnits, unitsToDecimal } from "@/lib/finance-money";
import { halfAway } from "@/lib/finance-instrument-policy";
import { calculateFinance } from "@/lib/finance-calculations";
import { financeCsv as serializeFinanceCsv } from "@/lib/finance-csv";
import { samePerson } from "@/lib/iam";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { financeTransaction } from "./finance-transactions";
import { requireFinanceTool } from "./finance-company-tools";
import { requireFinanceMoneyScale } from "./finance-money-policy";
import { executeCommand, replayCommand, assertion } from "./commands";
import { sha256 } from "./crypto";
import { ApiError } from "./http";
import { groupOwnershipWorkspace } from "./finance-group-ownership";
import { intercompanyPairWorkspace } from "./finance-intercompany-pairs";
import {
  intercompanyPairSchema,
  intercompanyPairSource,
} from "./finance-intercompany-pairs";
type Row = Record<string, string | number | null>;
const rate = z
  .string()
  .regex(/^\d+(\.\d{1,6})?$/)
  .refine((v) => decimalToUnits(v, 6) > BigInt(0));
const groupCode = z.string().regex(/^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$/);
const category = z.enum([
  "asset",
  "liability",
  "equity",
  "revenue",
  "expense",
  "other_income",
  "other_expense",
]);
const groupAccount = z.object({
  code: groupCode,
  name: z.string().trim().min(1).max(200),
  category,
});
export const groupReportEditionSchema = z
  .object({
    startDate: z.string().date(),
    endDate: z.string().date(),
    currency: z
      .string()
      .length(3)
      .transform((v) => v.toUpperCase()),
    parentId: z.string().min(1).optional(),
    parentVersion: z.number().int().positive().optional(),
    entities: z
      .array(
        z.object({
          companyId: z.string().min(1),
          chartId: z.string().min(1),
          periodIds: z.array(z.string().min(1)).min(1).max(24),
          method: z.enum(["full", "equity"]).default("full"),
        }),
      )
      .min(2)
      .max(20),
    mappings: z
      .array(
        z.object({
          accountId: z.string().min(1),
          group: groupAccount,
          fxType: z.enum(["current", "average", "historical"]),
          historicalRate: rate.optional(),
        }),
      )
      .min(1)
      .max(10000),
    rates: z
      .array(
        z.object({
          currency: z
            .string()
            .length(3)
            .transform((v) => v.toUpperCase()),
          current: rate,
          average: rate,
        }),
      )
      .max(50)
      .default([]),
    intercompanyPairIds: z.array(z.string().min(1)).max(500).default([]),
    acquisitionEliminations: z
      .array(
        z.object({
          acquisitionId: z.string().min(1),
          investmentAccountId: z.string().min(1),
          goodwill: groupAccount,
          nci: groupAccount,
          bargainPurchase: groupAccount,
        }),
      )
      .max(20)
      .default([]),
    adjustments: z
      .array(
        z.object({
          calculationRunId: z.string().min(1),
          ownershipId: z.string().min(1),
          acquisitionId: z.string().min(1).optional(),
          output: z.enum([
            "goodwill",
            "bargainPurchase",
            "nciClosing",
            "nciProfitShare",
            "equityMethodClosing",
          ]),
          debit: groupAccount,
          credit: groupAccount,
          existingDebitBalance: z
            .string()
            .regex(/^-?\d+(\.\d{1,2})?$/)
            .default("0.00"),
        }),
      )
      .max(100)
      .default([]),
    reason: z.string().trim().min(3).max(2000),
  })
  .superRefine((v, c) => {
    for (const [path, values] of [["intercompanyPairIds", v.intercompanyPairIds], ["acquisitionEliminations", v.acquisitionEliminations.map(a => a.acquisitionId)], ["adjustments", v.adjustments.map(a => `${a.ownershipId}:${a.output}`)]] as const) {
      if (new Set(values).size !== values.length) c.addIssue({code:"custom",path:[path],message:"Retain each document pair, acquisition or ownership adjustment once."});
    }
    if (v.startDate > v.endDate)
      c.addIssue({
        code: "custom",
        path: ["endDate"],
        message: "The reporting end must follow its start.",
      });
    if (new Set(v.entities.map((e) => e.companyId)).size !== v.entities.length)
      c.addIssue({
        code: "custom",
        path: ["entities"],
        message: "Select one explicit book per Company.",
      });
    if (new Set(v.mappings.map((m) => m.accountId)).size !== v.mappings.length)
      c.addIssue({
        code: "custom",
        path: ["mappings"],
        message: "Map each account once.",
      });
    if (new Set(v.rates.map((r) => r.currency)).size !== v.rates.length)
      c.addIssue({
        code: "custom",
        path: ["rates"],
        message: "Provide each currency's rates once.",
      });
  });
export const groupReportDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});
type Input = z.infer<typeof groupReportEditionSchema>;
function holdingActor(actor: WorkContext, permission: string) {
  actor = requirePermission(actor, permission);
  if (actor.scope.kind !== "holding")
    throw new ApiError(
      403,
      "HOLDING_FINANCE_REQUIRED",
      "Group reporting requires Holding Finance authority.",
    );
  return actor;
}
const profitCategory = (c: string) =>
  ["revenue", "expense", "other_income", "other_expense"].includes(c);
function convert(amount: bigint, fx: bigint) {
  return halfAway(amount * fx, BigInt(1000000));
}
async function source(db: SqlDatabase, actor: WorkContext, input: Input) {
  await requireFinanceMoneyScale(db, actor.companyId, input.currency);
  const definitions = new Map<string, z.infer<typeof groupAccount>>(),
    cumulative = new Map<string, bigint>(),
    interval = new Map<string, bigint>(),
    entities = [],
    adjustments = [],
    eliminations = [],
    acquisitions = [];
  let nciProfitAttribution = BigInt(0);
  function register(g: z.infer<typeof groupAccount>) {
    const old = definitions.get(g.code);
    if (old && JSON.stringify(old) !== JSON.stringify(g))
      throw new ApiError(
        422,
        "GROUP_ACCOUNT_CONFLICT",
        "Every group code must retain the same name and category.",
      );
    definitions.set(g.code, g);
  }
  function add(
    map: Map<string, bigint>,
    g: z.infer<typeof groupAccount>,
    amount: bigint,
  ) {
    register(g);
    map.set(g.code, (map.get(g.code) ?? BigInt(0)) + amount);
  }
  const accountMappings = new Map(input.mappings.map((m) => [m.accountId, m]));
  for (const entity of input.entities) {
    const registration = await db
      .prepare(
        "SELECT e.id,e.company_id,e.reporting_name,e.functional_currency,e.version,c.name chart_name,c.base_currency,c.status chart_status FROM finance_group_entities e JOIN finance_charts c ON c.company_id=e.company_id WHERE e.holding_id=? AND e.company_id=? AND e.status='active' AND c.id=?",
      )
      .bind(actor.holdingId, entity.companyId, entity.chartId)
      .first<Row>();
    if (
      !registration ||
      registration.chart_status !== "active" ||
      registration.functional_currency !== registration.base_currency
    )
      throw new ApiError(
        422,
        "GROUP_BOOK_REQUIRED",
        "Select an active registered Company book with its registered functional currency.",
      );
    await requireFinanceMoneyScale(
      db,
      entity.companyId,
      String(registration.base_currency),
    );
    const periods = (
      await db
        .prepare(
          `SELECT id,start_date,end_date,state,close_version FROM finance_periods WHERE company_id=? AND id IN (${entity.periodIds.map(() => "?").join(",")}) ORDER BY start_date,id`,
        )
        .bind(entity.companyId, ...entity.periodIds)
        .all<Row>()
    ).results;
    if (
      periods.length !== entity.periodIds.length ||
      periods.some((p) => p.state !== "closed") ||
      periods[0]?.start_date !== input.startDate ||
      periods.at(-1)?.end_date !== input.endDate ||
      periods.some(
        (p, i) =>
          i > 0 &&
          new Date(Date.parse(String(periods[i - 1].end_date)) + 86400000)
            .toISOString()
            .slice(0, 10) !== p.start_date,
      )
    )
      throw new ApiError(
        409,
        "GROUP_PERIODS_NOT_ALIGNED",
        "Every Company's closed periods must cover the exact reporting interval without gaps or overlaps.",
      );
    const lines = (
      await db
        .prepare(
          `SELECT l.id,l.account_id,l.debit_minor,l.credit_minor,l.base_debit_minor,l.base_credit_minor,l.dimension_json,j.id journal_id,j.version,j.entry_date,j.status,j.source_type,CASE WHEN ${fiscalCloseSourceSql()} THEN 1 ELSE 0 END is_fiscal_close,j.source_id,j.currency,j.memo,a.code,a.name,a.category,a.version account_version FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id JOIN finance_accounts a ON a.id=l.account_id WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND j.entry_date<=? ORDER BY j.entry_date,j.id,l.ordinal LIMIT 100001`,
        )
        .bind(entity.companyId, entity.chartId, input.endDate)
        .all<Row>()
    ).results.map(({ is_fiscal_close, ...line }) =>
      Number(is_fiscal_close) === 1 && line.source_type === "book_projection"
        ? { ...line, is_fiscal_close: 1 }
        : line,
    );
    if (lines.length > 100000)
      throw new ApiError(
        422,
        "GROUP_SOURCE_SIZE",
        "This book exceeds the current retained-source limit. Split the source adapter before preparing this edition.",
      );
    const functional = String(registration.base_currency),
      fx = input.rates.find((r) => r.currency === functional),
      byAccount = new Map<
        string,
        { cumulative: bigint; interval: bigint; category: string }
      >();
    for (const l of lines) {
      const m = accountMappings.get(String(l.account_id));
      if (!m || m.group.category !== l.category)
        throw new ApiError(
          422,
          "GROUP_MAPPING_REQUIRED",
          "Map every retained posted account to the matching group category.",
        );
      let exchange = BigInt(1000000);
      if (functional !== input.currency) {
        const selected =
          m.fxType === "historical" ? m.historicalRate : fx?.[m.fxType];
        if (!selected)
          throw new ApiError(
            422,
            "GROUP_RATE_REQUIRED",
            "Provide Current, Average and mapped Historical rates for each foreign source.",
          );
        exchange = decimalToUnits(selected, 6);
      }
      const raw = BigInt(l.base_debit_minor!) - BigInt(l.base_credit_minor!),
        amount = convert(raw, exchange),
        previous = byAccount.get(String(l.account_id)) ?? {
          cumulative: BigInt(0),
          interval: BigInt(0),
          category: String(l.category),
        };
      previous.cumulative += amount;
      if (
        String(l.entry_date) >= input.startDate &&
        !capturedFiscalClose(l)
      )
        previous.interval += amount;
      byAccount.set(String(l.account_id), previous);
    }
    const rawBalance = lines.reduce(
      (s, l) => s + BigInt(l.base_debit_minor!) - BigInt(l.base_credit_minor!),
      BigInt(0),
    );
    if (rawBalance !== BigInt(0))
      throw new ApiError(
        409,
        "GROUP_SOURCE_UNBALANCED",
        "The selected Company book does not balance at the cutoff.",
      );
    const translated = [...byAccount.values()].reduce(
        (s, a) => s + a.cumulative,
        BigInt(0),
      ),
      profit = -[...byAccount.values()]
        .filter((a) => profitCategory(a.category))
        .reduce((s, a) => s + a.interval, BigInt(0));
    if (entity.method === "full") {
      for (const [id, b] of byAccount) {
        const m = accountMappings.get(id)!;
        if (profitCategory(b.category)) add(interval, m.group, b.interval);
        else add(cumulative, m.group, b.cumulative);
      }
      const openProfit = [...byAccount.values()]
        .filter((a) => profitCategory(a.category))
        .reduce((s, a) => s + a.cumulative, BigInt(0));
      add(
        cumulative,
        {
          code: "_unclosed_profit",
          name: "Unclosed Company profit",
          category: "equity",
        },
        openProfit,
      );
      add(
        cumulative,
        {
          code: "_cta",
          name: "Currency translation adjustment",
          category: "equity",
        },
        -translated,
      );
    }
    entities.push({
      definition: entity,
      registration,
      periods,
      lines,
      profit: unitsToDecimal(profit),
      translatedUnbalance: unitsToDecimal(translated),
      accounts: [...byAccount].map(([accountId, b]) => ({
        accountId,
        category: b.category,
        cumulative: unitsToDecimal(b.cumulative),
        interval: unitsToDecimal(b.interval),
      })),
    });
  }
  if (
    new Set(input.intercompanyPairIds).size !== input.intercompanyPairIds.length
  )
    throw new ApiError(
      422,
      "GROUP_INTERCOMPANY_DUPLICATE",
      "Select each confirmed document pair once.",
    );
  for (const id of input.intercompanyPairIds) {
    const pair = await db
      .prepare(
        "SELECT * FROM finance_intercompany_document_pairs WHERE id=? AND holding_id=? AND status='confirmed'",
      )
      .bind(id, actor.holdingId)
      .first<Row>();
    if (!pair)
      throw new ApiError(
        409,
        "GROUP_INTERCOMPANY_NOT_CONFIRMED",
        "Use independently confirmed intercompany document pairs.",
      );
    const definition = intercompanyPairSchema.parse(
        JSON.parse(String(pair.source_json)).input,
      ),
      proof = await intercompanyPairSource(db, actor, definition);
    if (proof.hash !== pair.source_hash)
      throw new ApiError(
        409,
        "GROUP_INTERCOMPANY_SOURCE_CHANGED",
        "A paired document changed. Reverse the pairing and review new evidence.",
      );
    const lines = [];
    for (const [i, s] of [definition.source, definition.target].entries()) {
      const e = entities.find((e) => e.definition.companyId === s.companyId),
        journal = proof.evidence[i].journal,
        m = accountMappings.get(s.accountId);
      if (
        !e ||
        e.definition.method !== "full" ||
        e.definition.chartId !== journal.chart_id ||
        !m ||
        String(journal.entry_date) > input.endDate
      )
        throw new ApiError(
          409,
          "GROUP_INTERCOMPANY_BOOK_REQUIRED",
          "Both paired documents must belong to selected full-consolidation books at the reporting cutoff.",
        );
      if (
        profitCategory(m.group.category) &&
        String(journal.entry_date) < input.startDate
      )
        throw new ApiError(
          409,
          "GROUP_INTERCOMPANY_INTERVAL_REQUIRED",
          "Income and expense document pairs must belong to this reporting interval.",
        );
      const functional = String(e.registration.base_currency),
        rate = input.rates.find((r) => r.currency === functional),
        fx =
          functional === input.currency
            ? BigInt(1000000)
            : decimalToUnits(
                (m.fxType === "historical"
                  ? m.historicalRate
                  : rate?.[m.fxType])!,
                6,
              ),
        amount = -proof.evidence[i].lines.reduce(
          (sum, l) =>
            sum +
            convert(
              BigInt(l.base_debit_minor!) - BigInt(l.base_credit_minor!),
              fx,
            ),
          BigInt(0),
        );
      lines.push({ account: m.group, amount });
    }
    if (lines.reduce((s, l) => s + l.amount, BigInt(0)) !== BigInt(0))
      throw new ApiError(
        409,
        "GROUP_INTERCOMPANY_TRANSLATION_DIFFERENCE",
        "Paired translated amounts differ. Reconcile the Company FX or mapped translation policy before eliminating.",
      );
    for (const l of lines)
      add(
        profitCategory(l.account.category) ? interval : cumulative,
        l.account,
        l.amount,
      );
    eliminations.push({
      pair,
      proof: proof.evidence,
      lines: lines.map((l) => ({
        account: l.account,
        amount: unitsToDecimal(l.amount),
      })),
    });
  }
  if (
    new Set(input.acquisitionEliminations.map((a) => a.acquisitionId)).size !==
    input.acquisitionEliminations.length
  )
    throw new ApiError(
      422,
      "GROUP_ACQUISITION_DUPLICATE",
      "Select each acquisition elimination once.",
    );
  for (const a of input.acquisitionEliminations) {
    const acquisition = await db
      .prepare(
        "SELECT * FROM finance_acquisitions WHERE id=? AND holding_id=? AND company_id=? AND status='approved' AND currency=? AND acquisition_date<=?",
      )
      .bind(
        a.acquisitionId,
        actor.holdingId,
        actor.companyId,
        input.currency,
        input.endDate,
      )
      .first<Row>();
    if (!acquisition || !acquisition.ownership_id)
      throw new ApiError(
        409,
        "GROUP_ACQUISITION_SOURCE_REQUIRED",
        "Select approved owned acquisition evidence in the presentation currency.",
      );
    const ownership = await db
        .prepare(
          "SELECT o.*,i.company_id investor_company,v.company_id investee_company FROM finance_ownership_interests o JOIN finance_group_entities i ON i.id=o.investor_entity_id JOIN finance_group_entities v ON v.id=o.investee_entity_id WHERE o.id=? AND o.holding_id=? AND o.status IN ('approved','retired') AND o.approved_by IS NOT NULL",
        )
        .bind(acquisition.ownership_id, actor.holdingId)
        .first<Row>(),
      investee = entities.find(
        (e) => e.definition.companyId === ownership?.investee_company,
      ),
      investor = entities.find(
        (e) => e.definition.companyId === ownership?.investor_company,
      ),
      mapping = accountMappings.get(a.investmentAccountId);
    if (
      !ownership ||
      !investee ||
      !investor ||
      investee.definition.method !== "full" ||
      investor.definition.method !== "full" ||
      String(ownership.effective_from) > String(acquisition.acquisition_date) ||
      (ownership.effective_to &&
        String(ownership.effective_to) <
          String(acquisition.acquisition_date)) ||
      !mapping ||
      mapping.group.category !== "asset" ||
      !investor.accounts.some((b) => b.accountId === a.investmentAccountId) ||
      a.goodwill.category !== "asset" ||
      a.nci.category !== "equity" ||
      a.bargainPurchase.category !== "other_income"
    )
      throw new ApiError(
        409,
        "GROUP_ACQUISITION_ACCOUNTS_REQUIRED",
        "Bind an investor investment asset, subsidiary equity, goodwill asset, NCI equity and bargain-purchase income.",
      );
    if (
      new Set([
        mapping.group.code,
        a.goodwill.code,
        a.nci.code,
        a.bargainPurchase.code,
      ]).size !== 4
    )
      throw new ApiError(
        422,
        "GROUP_ACQUISITION_ACCOUNT_CONFLICT",
        "Keep investment, goodwill, NCI and bargain-purchase group accounts distinct.",
      );
    const consideration =
        BigInt(acquisition.consideration_minor!) +
        BigInt(acquisition.previous_interest_minor!),
      investorCarrying = decimalToUnits(
        investor.accounts.find((b) => b.accountId === a.investmentAccountId)!
          .cumulative,
      );
    if (investorCarrying !== consideration)
      throw new ApiError(
        409,
        "GROUP_INVESTMENT_CARRYING_MISMATCH",
        "The selected investor investment carrying balance must match reviewed consideration and previous interest.",
      );
    const functional = String(investee.registration.base_currency),
      rates = input.rates.find((r) => r.currency === functional),
      equityAtAcquisition = new Map<string, bigint>();
    for (const l of investee.lines.filter(
      (l) => String(l.entry_date) <= String(acquisition.acquisition_date),
    )) {
      const m = accountMappings.get(String(l.account_id))!;
      if (l.category !== "equity" && !profitCategory(String(l.category)))
        continue;
      const fx =
          functional === input.currency
            ? BigInt(1000000)
            : decimalToUnits(
                (m.fxType === "historical"
                  ? m.historicalRate
                  : rates?.[m.fxType])!,
                6,
              ),
        amount = convert(
          BigInt(l.base_debit_minor!) - BigInt(l.base_credit_minor!),
          fx,
        ),
        code = l.category === "equity" ? m.group.code : "_unclosed_profit";
      equityAtAcquisition.set(
        code,
        (equityAtAcquisition.get(code) ?? BigInt(0)) + amount,
      );
    }
    const net =
        BigInt(acquisition.identifiable_net_assets_minor!) -
        BigInt(acquisition.deferred_tax_minor!),
      sourceNet = -[...equityAtAcquisition.values()].reduce(
        (s, v) => s + v,
        BigInt(0),
      );
    if (sourceNet !== net)
      throw new ApiError(
        409,
        "GROUP_ACQUISITION_EQUITY_MISMATCH",
        "Reviewed identifiable net assets after deferred tax must reconcile to the translated subsidiary equity at acquisition. Review fair-value adjustments before preparing.",
      );
    const calc = await db
      .prepare(
        "SELECT * FROM finance_calculation_runs WHERE id=? AND holding_id=? AND company_id=? AND status='approved' AND kind='consolidation'",
      )
      .bind(acquisition.calculation_run_id, actor.holdingId, actor.companyId)
      .first<Row>();
    if (!calc)
      throw new ApiError(
        409,
        "GROUP_ACQUISITION_CALCULATION_REQUIRED",
        "The acquisition's owned calculation must remain approved.",
      );
    const data = JSON.parse(String(calc.input_json)),
      result = calculateFinance("consolidation", data, 2),
      hash = await sha256(
        JSON.stringify({
          kind: "consolidation",
          input: data,
          policyId: calc.policy_id ?? null,
          policyVersion: Number(calc.policy_version),
          currency: calc.currency,
          scale: Number(calc.currency_scale),
        }),
      );
    if (
      hash !== calc.source_hash ||
      JSON.stringify(result) !== calc.result_json ||
      Number(data.parentBps) !== Number(ownership.ownership_bps) ||
      decimalToUnits(String(data.consideration)) !==
        BigInt(acquisition.consideration_minor!) ||
      decimalToUnits(String(data.previousInterest)) !==
        BigInt(acquisition.previous_interest_minor!) ||
      decimalToUnits(String(data.fairValueNetAssets)) !== net ||
      decimalToUnits(String(data.nciFairValue)) !==
        BigInt(acquisition.nci_minor!)
    )
      throw new ApiError(
        409,
        "GROUP_ACQUISITION_CALCULATION_CHANGED",
        "The acquisition calculation must match all retained reviewed inputs.",
      );
    const goodwill = decimalToUnits(String(result.goodwill)),
      bargain = decimalToUnits(String(result.bargainPurchase));
    if (
      goodwill !== BigInt(acquisition.goodwill_minor!) ||
      bargain !== BigInt(acquisition.bargain_purchase_minor!)
    )
      throw new ApiError(
        409,
        "GROUP_ACQUISITION_CALCULATION_CHANGED",
        "Goodwill or bargain-purchase outputs changed.",
      );
    const postings = [];
    for (const [code, amount] of equityAtAcquisition) {
      const account = definitions.get(code)!;
      add(cumulative, account, -amount);
      postings.push({ account, amount: unitsToDecimal(-amount) });
    }
    add(cumulative, mapping.group, -consideration);
    add(cumulative, a.goodwill, goodwill);
    add(cumulative, a.nci, -BigInt(acquisition.nci_minor!));
    add(interval, a.bargainPurchase, -bargain);
    add(
      cumulative,
      {
        code: "_unclosed_profit",
        name: "Unclosed Company profit",
        category: "equity",
      },
      -bargain,
    );
    acquisitions.push({
      definition: a,
      acquisition,
      ownership,
      calculation: calc,
      postings: [
        ...postings,
        { account: mapping.group, amount: unitsToDecimal(-consideration) },
        { account: a.goodwill, amount: unitsToDecimal(goodwill) },
        {
          account: a.nci,
          amount: unitsToDecimal(-BigInt(acquisition.nci_minor!)),
        },
        { account: a.bargainPurchase, amount: unitsToDecimal(-bargain) },
      ],
    });
  }
  for (const a of input.adjustments) {
    if (["goodwill", "bargainPurchase"].includes(a.output))
      throw new ApiError(
        422,
        "GROUP_ACQUISITION_ELIMINATION_REQUIRED",
        "Use a complete acquisition elimination to remove investment and acquisition-date subsidiary equity together with goodwill and NCI.",
      );
    const calc = await db
        .prepare(
          "SELECT * FROM finance_calculation_runs WHERE id=? AND holding_id=? AND company_id=? AND kind='consolidation' AND status='approved' AND currency=? AND currency_scale=2",
        )
        .bind(
          a.calculationRunId,
          actor.holdingId,
          actor.companyId,
          input.currency,
        )
        .first<Row>(),
      ownership = await db
        .prepare(
          "SELECT o.*,v.company_id investee_company,i.company_id investor_company FROM finance_ownership_interests o JOIN finance_group_entities v ON v.id=o.investee_entity_id JOIN finance_group_entities i ON i.id=o.investor_entity_id WHERE o.id=? AND o.holding_id=? AND o.approved_by IS NOT NULL AND o.status IN ('approved','retired') AND o.effective_from<=? AND (o.effective_to IS NULL OR o.effective_to>=?)",
        )
        .bind(a.ownershipId, actor.holdingId, input.startDate, input.endDate)
        .first<Row>();
    if (!calc || !ownership)
      throw new ApiError(
        409,
        "GROUP_OWNERSHIP_CALCULATION_REQUIRED",
        "Use an approved owned consolidation calculation and effective approved ownership evidence.",
      );
    const data = JSON.parse(String(calc.input_json)) as Record<string, unknown>,
      result = calculateFinance("consolidation", data, 2),
      sourceEntity = entities.find(
        (e) => e.definition.companyId === ownership.investee_company,
      );
    if (
      !sourceEntity ||
      !entities.some(
        (e) => e.definition.companyId === ownership.investor_company,
      ) ||
      JSON.stringify(result) !== calc.result_json
    )
      throw new ApiError(
        409,
        "GROUP_CALCULATION_CHANGED",
        "The calculation must reproduce its retained result and reference selected entities.",
      );
    const calcHash = await sha256(
      JSON.stringify({
        kind: "consolidation",
        input: data,
        policyId: calc.policy_id ?? null,
        policyVersion: Number(calc.policy_version),
        currency: calc.currency,
        scale: Number(calc.currency_scale),
      }),
    );
    if (calcHash !== calc.source_hash)
      throw new ApiError(
        409,
        "GROUP_CALCULATION_CHANGED",
        "The approved calculation input no longer matches its retained checksum.",
      );
    const equity = a.output === "equityMethodClosing",
      acquisitionOutput = ["goodwill", "bargainPurchase"].includes(a.output);
    if (
      ownership.accounting_method !== (equity ? "equity" : "subsidiary") ||
      Number(data[equity ? "associateBps" : "parentBps"]) !==
        Number(ownership.ownership_bps) ||
      (!acquisitionOutput &&
        decimalToUnits(
          String(data[equity ? "associateProfit" : "subsidiaryProfit"]),
        ) !== decimalToUnits(sourceEntity.profit)) ||
      sourceEntity.definition.method !== (equity ? "equity" : "full")
    )
      throw new ApiError(
        409,
        "GROUP_CALCULATION_SOURCE_MISMATCH",
        "Approved ownership percentage and actual translated Company profit must match the calculation inputs.",
      );
    let acquisition: Row | null = null;
    if (["goodwill", "bargainPurchase"].includes(a.output)) {
      acquisition = await db
        .prepare(
          "SELECT * FROM finance_acquisitions WHERE id=? AND holding_id=? AND investee_entity_id=? AND status IN ('approved','impaired') AND acquisition_date<=? AND currency=? AND calculation_run_id=?",
        )
        .bind(
          a.acquisitionId ?? "",
          actor.holdingId,
          ownership.investee_entity_id,
          input.endDate,
          input.currency,
          calc.id,
        )
        .first<Row>();
      if (
        !acquisition ||
        acquisition.ownership_id !== ownership.id ||
        decimalToUnits(String(data.consideration)) !==
          BigInt(acquisition.consideration_minor!) ||
        decimalToUnits(String(data.previousInterest ?? "0")) !==
          BigInt(acquisition.previous_interest_minor!) ||
        decimalToUnits(String(data.fairValueNetAssets)) !==
          BigInt(acquisition.identifiable_net_assets_minor!) -
            BigInt(acquisition.deferred_tax_minor!) ||
        decimalToUnits(String(data.nciFairValue)) !==
          BigInt(acquisition.nci_minor!) ||
        decimalToUnits(String(result.goodwill)) !==
          BigInt(acquisition.goodwill_minor!) ||
        decimalToUnits(String(result.bargainPurchase)) !==
          BigInt(acquisition.bargain_purchase_minor!)
      )
        throw new ApiError(
          409,
          "GROUP_ACQUISITION_SOURCE_MISMATCH",
          "Bind goodwill to the approved acquisition and its exact reviewed calculation.",
        );
    }
    if (a.output === "nciProfitShare") {
      nciProfitAttribution += decimalToUnits(String(result.nciProfitShare));
      adjustments.push({
        definition: a,
        calculation: calc,
        ownership,
        acquisition,
        amount: String(result.nciProfitShare),
      });
      continue;
    }
    const existing = decimalToUnits(a.existingDebitBalance),
      actual =
        a.output === "nciClosing"
          ? -(cumulative.get(a.credit.code) ?? BigInt(0))
          : (cumulative.get(a.debit.code) ?? BigInt(0));
    if (existing !== actual)
      throw new ApiError(
        409,
        "GROUP_ADJUSTMENT_CARRYING_CHANGED",
        "The retained equity-method debit or NCI credit carrying balance must match the selected consolidated source.",
      );
    if (
      a.output === "nciClosing" &&
      (a.debit.category !== "equity" || a.credit.category !== "equity")
    )
      throw new ApiError(
        422,
        "GROUP_NCI_ACCOUNTS_REQUIRED",
        "NCI closing reallocates parent and noncontrolling equity without changing total group profit.",
      );
    const amount = decimalToUnits(String(result[a.output])) - existing;
    if (a.debit.code === a.credit.code)
      throw new ApiError(
        422,
        "GROUP_ADJUSTMENT_ACCOUNT_PAIR",
        "Choose distinct group adjustment accounts.",
      );
    const destination =
      profitCategory(a.debit.category) || profitCategory(a.credit.category)
        ? interval
        : cumulative;
    // A mixed BS/P&L adjustment also carries the P&L result into unclosed equity.
    add(
      profitCategory(a.debit.category) ? interval : cumulative,
      a.debit,
      amount,
    );
    add(
      profitCategory(a.credit.category) ? interval : cumulative,
      a.credit,
      -amount,
    );
    if (destination === interval) {
      const pl =
        (profitCategory(a.debit.category) ? amount : BigInt(0)) -
        (profitCategory(a.credit.category) ? amount : BigInt(0));
      add(
        cumulative,
        {
          code: "_unclosed_profit",
          name: "Unclosed Company profit",
          category: "equity",
        },
        pl,
      );
    }
    adjustments.push({
      definition: a,
      calculation: calc,
      ownership,
      acquisition,
      amount: unitsToDecimal(amount),
    });
  }
  for (const e of entities.filter((e) => e.definition.method === "equity"))
    if (
      !adjustments.some(
        (a) =>
          a.ownership.investee_company === e.definition.companyId &&
          a.definition.output === "equityMethodClosing",
      )
    )
      throw new ApiError(
        409,
        "GROUP_EQUITY_ADJUSTMENT_REQUIRED",
        "An excluded associate book requires its owned equity-method closing adjustment.",
      );
  if ([...cumulative.values()].reduce((s, a) => s + a, BigInt(0)) !== BigInt(0))
    throw new ApiError(
      409,
      "GROUP_BALANCE_SHEET_UNBALANCED",
      "Group balance-sheet sources and reviewed adjustments must balance.",
    );
  const profit = -[...interval.values()].reduce((s, a) => s + a, BigInt(0)),
    moneyRows = (rows: Map<string, bigint>) =>
      [...rows]
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([code, amount]) => ({
          ...definitions.get(code)!,
          amount: unitsToDecimal(amount),
        })),
    result = {
      currency: input.currency,
      startDate: input.startDate,
      endDate: input.endDate,
      balanceSheet: moneyRows(cumulative),
      profitAndLoss: moneyRows(interval),
      profit: unitsToDecimal(profit),
      profitAttribution: {
        parent: unitsToDecimal(profit - nciProfitAttribution),
        nci: unitsToDecimal(nciProfitAttribution),
      },
    };
  const evidence = { entities, adjustments, eliminations, acquisitions },
    hash = await sha256(JSON.stringify({ input, evidence, result }));
  return { evidence, result, hash };
}
async function get(db: SqlDatabase, actor: WorkContext, id: string) {
  const r = await db
    .prepare(
      "SELECT * FROM finance_group_report_editions WHERE id=? AND holding_id=? AND company_id=?",
    )
    .bind(id, actor.holdingId, actor.companyId)
    .first<Row>();
  if (!r)
    throw new ApiError(
      404,
      "GROUP_EDITION_NOT_FOUND",
      "Group reporting edition not found.",
    );
  return r;
}
export async function prepareGroupReportEdition(
  actor: WorkContext,
  raw: z.input<typeof groupReportEditionSchema>,
  key: string,
) {
  actor = holdingActor(actor, "finance.write");
  const input = groupReportEditionSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    id = `group-edition-${key}`,
    op = "finance.group-edition.prepare";
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    await requireFinanceTool(db, actor.companyId, "group_reporting");
    const parent = input.parentId ? await get(db, actor, input.parentId) : null;
    if (
      parent &&
      (parent.status !== "approved" ||
        parent.version !== input.parentVersion ||
        parent.company_id !== actor.companyId ||
        parent.start_date !== input.startDate ||
        parent.end_date !== input.endDate ||
        parent.currency !== input.currency)
    )
      throw new ApiError(
        409,
        "GROUP_EDITION_PARENT_CHANGED",
        "Amend the current approved edition for the same report interval and presentation currency.",
      );
    const e = await source(db, actor, input),
      csv = serializeFinanceCsv(
        ["statement", "groupCode", "name", "category", "amount", "currency"],
        [
          ...e.result.balanceSheet.map((r) => [
            "balance_sheet",
            r.code,
            r.name,
            r.category,
            r.amount,
            input.currency,
          ]),
          ...e.result.profitAndLoss.map((r) => [
            "profit_and_loss",
            r.code,
            r.name,
            r.category,
            r.amount,
            input.currency,
          ]),
          ["profit_attribution", "parent", "Profit attributable to parent", "income", e.result.profitAttribution.parent, input.currency],
          ["profit_attribution", "nci", "Profit attributable to non-controlling interests", "income", e.result.profitAttribution.nci, input.currency],
        ],
      ),
      exportHash = await sha256(csv),
      now = Date.now();
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      1,
      { id, status: "draft", version: 1, sourceHash: e.hash },
      [
        db
          .prepare(
            "INSERT INTO finance_group_report_editions(id,holding_id,company_id,parent_id,root_id,edition,start_date,end_date,currency,content_json,source_json,result_json,source_hash,export_content,export_sha256,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            id,
            actor.holdingId,
            actor.companyId,
            parent?.id ?? null,
            parent?.root_id ?? id,
            Number(parent?.edition ?? 0) + 1,
            input.startDate,
            input.endDate,
            input.currency,
            JSON.stringify(input),
            JSON.stringify(e.evidence),
            JSON.stringify(e.result),
            e.hash,
            csv,
            exportHash,
            actor.userId,
            input.reason,
            now,
            now,
          ),
      ],
    );
  });
}
export async function decideGroupReportEdition(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof groupReportDecisionSchema>,
  action: "approve" | "reject",
  key: string,
) {
  actor = holdingActor(actor, "finance.approve");
  const input = groupReportDecisionSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    op = `finance.group-edition.${action}`;
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    const r = await get(db, actor, id);
    if (r.status !== "draft" || r.version !== input.expectedVersion)
      throw new ApiError(
        409,
        "GROUP_EDITION_CHANGED",
        "Refresh the draft before deciding.",
      );
    if (samePerson(actor, r.prepared_by))
      throw new ApiError(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Holding Finance approver must review the group edition.",
      );
    const definition = groupReportEditionSchema.parse(
        JSON.parse(String(r.content_json)),
      ),
      e = action === "approve" ? await source(db, actor, definition) : null;
    if (e && e.hash !== r.source_hash)
      throw new ApiError(
        409,
        "GROUP_EDITION_SOURCE_CHANGED",
        "Period, ledger, mapping, rate or adjustment evidence changed. Reject and prepare again.",
      );
    const steps = [];
    if (action === "approve" && r.parent_id) {
      const p = await get(db, actor, String(r.parent_id));
      if (p.status !== "approved" || p.version !== definition.parentVersion)
        throw new ApiError(
          409,
          "GROUP_EDITION_PARENT_CHANGED",
          "The earlier edition changed before amendment approval.",
        );
      steps.push(
        db
          .prepare(
            "UPDATE finance_group_report_editions SET status='superseded',version=version+1,updated_at=? WHERE id=?",
          )
          .bind(Date.now(), p.id),
      );
    }
    steps.push(
      assertion(
        db,
        key,
        "group-edition-version",
        "EXISTS(SELECT 1 FROM finance_group_report_editions WHERE id=? AND holding_id=? AND status='draft' AND version=?)",
        [id, actor.holdingId, input.expectedVersion],
      ),
      db
        .prepare(
          "UPDATE finance_group_report_editions SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(
          action === "approve" ? "approved" : "rejected",
          actor.userId,
          Date.now(),
          id,
        ),
    );
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      input.expectedVersion + 1,
      {
        id,
        status: action === "approve" ? "approved" : "rejected",
        version: input.expectedVersion + 1,
      },
      steps,
    );
  });
}
export async function groupReportEditionExport(actor: WorkContext, id: string) {
  actor = holdingActor(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId),
    r = await get(write, actor, id);
  if (!["approved", "superseded"].includes(String(r.status)))
    throw new ApiError(
      409,
      "GROUP_EDITION_NOT_APPROVED",
      "Approve this group reporting edition before exporting.",
    );
  if ((await sha256(String(r.export_content))) !== r.export_sha256)
    throw new ApiError(
      409,
      "GROUP_EXPORT_EVIDENCE_CHANGED",
      "Retained group export bytes differ from their checksum.",
    );
  return {
    content: String(r.export_content),
    sha256: String(r.export_sha256),
    filename: `group-${r.start_date}-${r.end_date}-${r.edition}.csv`,
  };
}
export async function groupEditionSourceJournal(
  actor: WorkContext,
  id: string,
  journalId: string,
) {
  actor = holdingActor(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId),
    edition = await get(write, actor, id),
    proof = JSON.parse(String(edition.source_json)) as {
      entities: { registration: Row; lines: Row[] }[];
    },
    entity = proof.entities.find((e) =>
      e.lines.some((l) => l.journal_id === journalId),
    );
  if (!entity)
    throw new ApiError(
      404,
      "GROUP_SOURCE_JOURNAL_NOT_FOUND",
      "The source journal is not included in this retained group edition.",
    );
  return {
    edition,
    entity: entity.registration,
    lines: entity.lines.filter((l) => l.journal_id === journalId),
  };
}
export async function groupReportEditionWorkspace(actor: WorkContext) {
  actor = holdingActor(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  return (
    await write
      .prepare(
        "SELECT * FROM finance_group_report_editions WHERE holding_id=? AND company_id=? ORDER BY created_at DESC,id LIMIT 100",
      )
      .bind(actor.holdingId, actor.companyId)
      .all<Row>()
  ).results;
}
export async function groupExpansionWorkspace(actor: WorkContext) {
  actor = holdingActor(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const [
    editions,
    ownership,
    pairs,
    charts,
    periods,
    accounts,
    documents,
    calculations,
  ] = await Promise.all([
    groupReportEditionWorkspace(actor),
    groupOwnershipWorkspace(actor),
    intercompanyPairWorkspace(actor),
    write
      .prepare(
        "SELECT c.*,e.reporting_name FROM finance_charts c JOIN finance_group_entities e ON e.company_id=c.company_id AND e.holding_id=? AND e.status='active' WHERE c.status='active' ORDER BY e.reporting_name,c.name,c.id",
      )
      .bind(actor.holdingId)
      .all<Row>(),
    write
      .prepare(
        "SELECT p.* FROM finance_periods p JOIN finance_group_entities e ON e.company_id=p.company_id AND e.holding_id=? AND e.status='active' WHERE p.state='closed' ORDER BY p.start_date,p.id",
      )
      .bind(actor.holdingId)
      .all<Row>(),
    write
      .prepare(
        "SELECT a.*,e.reporting_name FROM finance_accounts a JOIN finance_group_entities e ON e.company_id=a.company_id AND e.holding_id=? AND e.status='active' ORDER BY e.reporting_name,a.code,a.id",
      )
      .bind(actor.holdingId)
      .all<Row>(),
    write
      .prepare(
        "SELECT j.id journal_id,j.company_id,j.chart_id,j.entry_date,j.currency,j.source_type,j.source_id,j.memo,e.reporting_name,a.id account_id,a.code,a.name,a.category,SUM(l.debit_minor-l.credit_minor) amount_minor FROM finance_journals j JOIN finance_group_entities e ON e.company_id=j.company_id AND e.holding_id=? AND e.status='active' JOIN finance_journal_lines l ON l.journal_id=j.id JOIN finance_accounts a ON a.id=l.account_id WHERE j.status='posted' GROUP BY j.id,e.reporting_name,a.id HAVING SUM(l.debit_minor-l.credit_minor)<>0 ORDER BY j.entry_date DESC,j.id,a.code LIMIT 1000",
      )
      .bind(actor.holdingId)
      .all<Row>(),
    write
      .prepare(
        "SELECT * FROM finance_calculation_runs WHERE holding_id=? AND company_id=? AND kind='consolidation' AND status='approved' ORDER BY created_at DESC,id LIMIT 200",
      )
      .bind(actor.holdingId, actor.companyId)
      .all<Row>(),
  ]);
  return {
    editions,
    ...ownership,
    pairs,
    charts: charts.results,
    periods: periods.results,
    accounts: accounts.results,
    documents: documents.results,
    calculations: calculations.results,
  };
}

/** The individual drawer retains the owner's Holding Finance and Company bounds. */
async function groupRecordNames(db:SqlDatabase,row:Row,companyId:string){
  const fields=["created_by","prepared_by","reviewed_by","approved_by"],ids=[...new Set(fields.map(field=>row[field]).filter((id):id is string=>typeof id==="string"))];
  if(!ids.length)return row;
  const people=(await db.prepare(`SELECT user_id,legal_name FROM hr_people WHERE company_id=? AND merged_into_id IS NULL AND user_id IN (${ids.map(()=>"?").join(",")})`).bind(companyId,...ids).all<{user_id:string;legal_name:string}>()).results;
  return {...row,...Object.fromEntries(fields.filter(field=>row[field]).map(field=>[`${field}_name`,people.find(person=>person.user_id===row[field])?.legal_name??"Unlinked Finance account"]))};
}
export async function groupExpansionRecord(
  actor: WorkContext,
  kind:
    | "finance_group_edition"
    | "finance_group_ownership"
    | "finance_group_acquisition"
    | "finance_group_pair",
  id: string,
) {
  actor = holdingActor(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const table = {
    finance_group_edition: "finance_group_report_editions",
    finance_group_ownership: "finance_ownership_interests",
    finance_group_acquisition: "finance_acquisitions",
    finance_group_pair: "finance_intercompany_document_pairs",
  }[kind];
  const record = await write
    .prepare(
      `SELECT * FROM ${table} WHERE holding_id=? AND company_id=? AND id=?`,
    )
    .bind(actor.holdingId, actor.companyId, id)
    .first<Row>();
  if (!record)
    throw new ApiError(
      404,
      "GROUP_EVIDENCE_NOT_FOUND",
      "Group evidence not found in this Company.",
    );
  const workspace = await groupExpansionWorkspace(actor),
    field = {
      finance_group_edition: "editions",
      finance_group_ownership: "ownership",
      finance_group_acquisition: "acquisitions",
      finance_group_pair: "pairs",
    }[kind] as "editions" | "ownership" | "acquisitions" | "pairs";
  const row = workspace[field].find((row) => row.id === id) ?? record;
  return {
    row:await groupRecordNames(write,row,actor.companyId),
    groupData: {
      ...workspace,
      editions: [],
      ownership: [],
      acquisitions: [],
      pairs: [],
      [field]: [row],
    },
  };
}

/** Source pagination is over the immutable edition, never a live cross-Company query. */
export async function groupEditionAccountSources(
  actor: WorkContext,
  id: string,
  accountCode: string,
  page: number,
) {
  actor = holdingActor(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId),
    edition = await get(write, actor, id);
  const definition = JSON.parse(String(edition.content_json)) as Input;
  const result = JSON.parse(String(edition.result_json)) as {
    balanceSheet: { code: string; name: string; amount: string }[];
    profitAndLoss: { code: string; name: string; amount: string }[];
  };
  const account = [...result.balanceSheet, ...result.profitAndLoss].find(
    (row) => row.code === accountCode,
  );
  if (!account)
    throw new ApiError(
      404,
      "GROUP_ACCOUNT_NOT_FOUND",
      "Group account not found in this retained edition.",
    );
  const accountIds = new Set(
    definition.mappings
      .filter((mapping) => mapping.group.code === accountCode)
      .map((mapping) => mapping.accountId),
  );
  const proof = JSON.parse(String(edition.source_json)) as {
    entities: { definition: Input['entities'][number]; registration: Row; lines: Row[] }[];
    adjustments: unknown[]; eliminations: unknown[]; acquisitions: unknown[];
  };
  const interval = result.profitAndLoss.some(row => row.code === accountCode);
  const sources = proof.entities.filter(entity => entity.definition.method === 'full').flatMap((entity) =>
    entity.lines
      .filter((line) => {
        const included = accountIds.has(String(line.account_id)) || accountCode === '_cta' || accountCode === '_unclosed_profit' && profitCategory(String(line.category));
        return included && (!interval || String(line.entry_date) >= definition.startDate && !capturedFiscalClose(line));
      })
      .map((line) => ({
        ...line,
        name: `${entity.registration.reporting_name} · ${line.code} · ${line.name}`,
        currency: line.currency ?? entity.registration.base_currency,
      })),
  );
  const rows = sources.slice((page - 1) * 30, page * 30);
  return {
    row: {
      ...await groupRecordNames(write,edition,actor.companyId),
      name: `${account.code} · ${account.name}`,
      reported_total: `${edition.currency} ${account.amount}`,
      description:
        "Retained original journal amounts. The edition records currency translation and group adjustments separately.",
    },
    rows,
    page,
    hasNext: page * 30 < sources.length,
    sourceArgs: { editionId: id },
    calculationEvidence: { rates: definition.rates, mappings: definition.mappings.filter(mapping => accountIds.has(mapping.accountId)), adjustments: proof.adjustments, eliminations: proof.eliminations, acquisitions: proof.acquisitions },
  };
}
