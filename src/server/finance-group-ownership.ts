import "server-only";
import { z } from "zod";
import { decimalToUnits, unitsToDecimal } from "@/lib/finance-money";
import { halfAway } from "@/lib/finance-instrument-policy";
import { calculateFinance } from "@/lib/finance-calculations";
import { samePerson } from "@/lib/iam";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { financeTransaction } from "./finance-transactions";
import { requireFinanceTool } from "./finance-company-tools";
import { requireFinanceMoneyScale } from "./finance-money-policy";
import { assertion, executeCommand, replayCommand } from "./commands";
import { sha256 } from "./crypto";
import { ApiError } from "./http";
type Row = Record<string, string | number | null>;
const money = z.string().regex(/^\d+(\.\d{1,2})?$/),
  reason = z.string().trim().min(3).max(2000);
export const ownershipEditionSchema = z
  .object({
    investorCompanyId: z.string().min(1),
    investeeCompanyId: z.string().min(1),
    method: z.enum(["subsidiary", "equity", "cost", "excluded"]),
    ownershipBps: z.number().int().min(0).max(10000),
    votingBps: z.number().int().min(0).max(10000),
    controlAssessment: reason,
    effectiveFrom: z.string().date(),
    effectiveTo: z.string().date().optional(),
    parentId: z.string().min(1).optional(),
    parentVersion: z.number().int().positive().optional(),
    reason,
  })
  .superRefine((v, c) => {
    if (Boolean(v.parentId) !== Boolean(v.parentVersion))
      c.addIssue({
        code: "custom",
        path: ["parentVersion"],
        message: "Retain the exact ownership edition being revised.",
      });
    if (v.investorCompanyId === v.investeeCompanyId)
      c.addIssue({
        code: "custom",
        message: "Select different investor and investee Companies.",
      });
    if (v.effectiveTo && v.effectiveTo < v.effectiveFrom)
      c.addIssue({
        code: "custom",
        message: "The effective end cannot precede its start.",
      });
    if (v.method === "equity" && v.ownershipBps === 0)
      c.addIssue({
        code: "custom",
        message:
          "An equity-method interest needs a positive ownership percentage.",
      });
  });
export const acquisitionEditionSchema = z
  .object({
    parentId: z.string().min(1).optional(),
    parentVersion: z.number().int().positive().optional(),
    ownershipId: z.string().min(1),
    ownershipVersion: z.number().int().positive(),
    acquisitionDate: z.string().date(),
    currency: z
      .string()
      .length(3)
      .transform((v) => v.toUpperCase()),
    consideration: money,
    previousInterest: money.default("0"),
    nciMeasurement: z.enum(["fair_value", "proportionate"]),
    nciFairValue: money.default("0"),
    identifiableNetAssets: money,
    deferredTax: money.default("0"),
    cguReference: z.string().trim().min(1).max(200),
    reason,
  })
  .superRefine((input, context) => {
    if (Boolean(input.parentId) !== Boolean(input.parentVersion))
      context.addIssue({
        code: "custom",
        path: ["parentVersion"],
        message: "Retain the exact acquisition edition being corrected.",
      });
  });
export const groupEvidenceDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason,
});
function actorGuard(actor: WorkContext, permission: string) {
  actor = requirePermission(actor, permission);
  if (actor.scope.kind !== "holding")
    throw new ApiError(
      403,
      "HOLDING_FINANCE_REQUIRED",
      "Group ownership evidence requires Holding Finance authority.",
    );
  return actor;
}
async function get(
  db: SqlDatabase,
  actor: WorkContext,
  table: "finance_ownership_interests" | "finance_acquisitions",
  id: string,
) {
  const row = await db
    .prepare(
      `SELECT * FROM ${table} WHERE id=? AND holding_id=? AND company_id=?`,
    )
    .bind(id, actor.holdingId, actor.companyId)
    .first<Row>();
  if (!row)
    throw new ApiError(
      404,
      "GROUP_EVIDENCE_NOT_FOUND",
      "Owned group evidence was not found in this Company.",
    );
  return row;
}
async function ownershipSource(
  db: SqlDatabase,
  actor: WorkContext,
  input: z.infer<typeof ownershipEditionSchema>,
) {
  const entities = (
    await db
      .prepare(
        "SELECT * FROM finance_group_entities WHERE holding_id=? AND company_id IN (?,?) AND status='active' ORDER BY company_id",
      )
      .bind(actor.holdingId, input.investorCompanyId, input.investeeCompanyId)
      .all<Row>()
  ).results;
  if (entities.length !== 2)
    throw new ApiError(
      422,
      "GROUP_ENTITY_REQUIRED",
      "Register both active Companies before preparing ownership evidence.",
    );
  const parent = input.parentId
    ? await get(db, actor, "finance_ownership_interests", input.parentId)
    : null;
  if (
    parent &&
    (parent.status !== "approved" ||
      parent.version !== input.parentVersion ||
      parent.investor_entity_id !==
        entities.find((e) => e.company_id === input.investorCompanyId)?.id ||
      parent.investee_entity_id !==
        entities.find((e) => e.company_id === input.investeeCompanyId)?.id ||
      String(parent.effective_from) >= input.effectiveFrom ||
      (parent.effective_to &&
        String(parent.effective_to) < input.effectiveFrom))
  )
    throw new ApiError(
      409,
      "OWNERSHIP_PARENT_CHANGED",
      "Revise the current approved interest with the same entities and a later effective start.",
    );
  const overlap = await db
    .prepare(
      "SELECT 1 FROM finance_ownership_interests WHERE holding_id=? AND investor_entity_id=? AND investee_entity_id=? AND status='approved' AND id<>? AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?)",
    )
    .bind(
      actor.holdingId,
      entities.find((e) => e.company_id === input.investorCompanyId)!.id,
      entities.find((e) => e.company_id === input.investeeCompanyId)!.id,
      parent?.id ?? "",
      input.effectiveTo ?? "9999-12-31",
      input.effectiveFrom,
    )
    .first();
  if (overlap)
    throw new ApiError(
      409,
      "OWNERSHIP_EFFECTIVE_OVERLAP",
      "An approved interest already covers this date range. Prepare a dated revision.",
    );
  const evidence = { entities, parent },
    hash = await sha256(JSON.stringify({ input, evidence }));
  return { evidence, hash, parent };
}
export async function prepareOwnershipEdition(
  actor: WorkContext,
  raw: z.input<typeof ownershipEditionSchema>,
  key: string,
) {
  actor = actorGuard(actor, "finance.write");
  const input = ownershipEditionSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    id = `ownership-${key}`,
    op = "finance.ownership.prepare";
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    await requireFinanceTool(db, actor.companyId, "group_reporting");
    const e = await ownershipSource(db, actor, input),
      now = Date.now();
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      1,
      { id, status: "draft", version: 1 },
      [
        db
          .prepare(
            "INSERT INTO finance_ownership_interests(id,holding_id,company_id,parent_id,investor_entity_id,investee_entity_id,accounting_method,ownership_bps,voting_bps,control_assessment,effective_from,effective_to,created_by,created_at,updated_at,source_hash,source_json,reason) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            id,
            actor.holdingId,
            actor.companyId,
            input.parentId ?? null,
            e.evidence.entities.find(
              (v) => v.company_id === input.investorCompanyId,
            )!.id,
            e.evidence.entities.find(
              (v) => v.company_id === input.investeeCompanyId,
            )!.id,
            input.method,
            input.ownershipBps,
            input.votingBps,
            input.controlAssessment,
            input.effectiveFrom,
            input.effectiveTo ?? null,
            actor.userId,
            now,
            now,
            e.hash,
            JSON.stringify({ input, evidence: e.evidence }),
            input.reason,
          ),
      ],
    );
  });
}
export async function decideOwnershipEdition(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof groupEvidenceDecisionSchema>,
  action: "approve" | "reject",
  key: string,
) {
  actor = actorGuard(actor, "finance.approve");
  const input = groupEvidenceDecisionSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    op = `finance.ownership.${action}`;
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    const r = await get(db, actor, "finance_ownership_interests", id);
    if (
      r.status !== "draft" ||
      r.version !== input.expectedVersion ||
      !r.source_json
    )
      throw new ApiError(
        409,
        "OWNERSHIP_CHANGED",
        "Review the current owned ownership draft.",
      );
    if (samePerson(actor, r.created_by))
      throw new ApiError(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Finance approver must review ownership evidence.",
      );
    const definition = ownershipEditionSchema.parse(
        JSON.parse(String(r.source_json)).input,
      ),
      e =
        action === "approve"
          ? await ownershipSource(db, actor, definition)
          : null;
    if (e && e.hash !== r.source_hash)
      throw new ApiError(
        409,
        "OWNERSHIP_SOURCE_CHANGED",
        "Entity or earlier ownership evidence changed. Reject and prepare again.",
      );
    const now = Date.now(),
      steps = [];
    if (e?.parent) {
      const end = new Date(Date.parse(definition.effectiveFrom) - 86400000)
        .toISOString()
        .slice(0, 10);
      steps.push(
        db
          .prepare(
            "UPDATE finance_ownership_interests SET status='retired',effective_to=?,version=version+1,updated_at=? WHERE id=? AND version=?",
          )
          .bind(end, now, e.parent.id, e.parent.version),
      );
    }
    steps.push(
      assertion(
        db,
        key,
        "ownership-version",
        "EXISTS(SELECT 1 FROM finance_ownership_interests WHERE id=? AND holding_id=? AND company_id=? AND status='draft' AND version=?)",
        [id, actor.holdingId, actor.companyId, input.expectedVersion],
      ),
      db
        .prepare(
          "UPDATE finance_ownership_interests SET status=?,approved_by=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(
          action === "approve" ? "approved" : "rejected",
          actor.userId,
          now,
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
async function acquisitionSource(
  db: SqlDatabase,
  actor: WorkContext,
  input: z.infer<typeof acquisitionEditionSchema>,
  currentId = "",
) {
  await requireFinanceMoneyScale(db, actor.companyId, input.currency);
  const ownership = await get(
    db,
    actor,
    "finance_ownership_interests",
    input.ownershipId,
  );
  if (
    ownership.version !== input.ownershipVersion ||
    !["approved", "retired"].includes(String(ownership.status)) ||
    !ownership.approved_by ||
    ownership.accounting_method !== "subsidiary" ||
    String(ownership.effective_from) > input.acquisitionDate ||
    (ownership.effective_to &&
      String(ownership.effective_to) < input.acquisitionDate)
  )
    throw new ApiError(
      409,
      "ACQUISITION_OWNERSHIP_REQUIRED",
      "Select the effective approved subsidiary interest and its exact version.",
    );
  const parent = input.parentId
    ? await get(db, actor, "finance_acquisitions", input.parentId)
    : null;
  if (
    parent &&
    (parent.status !== "approved" ||
      parent.version !== input.parentVersion ||
      parent.ownership_id !== input.ownershipId ||
      parent.acquisition_date !== input.acquisitionDate ||
      parent.currency !== input.currency ||
      !parent.source_hash)
  )
    throw new ApiError(
      409,
      "ACQUISITION_PARENT_CHANGED",
      "Correct the current approved acquisition with the same interest, acquisition date and currency. A new purchase needs its own dated ownership evidence.",
    );
  if (
    await db
      .prepare(
        "SELECT 1 FROM finance_acquisitions WHERE company_id=? AND ownership_id=? AND acquisition_date=? AND currency=? AND source_hash IS NOT NULL AND status IN ('draft','approved','impaired') AND id NOT IN (?,?)",
      )
      .bind(
        actor.companyId,
        input.ownershipId,
        input.acquisitionDate,
        input.currency,
        parent?.id ?? "",
        currentId,
      )
      .first()
  )
    throw new ApiError(
      409,
      "ACQUISITION_REVISION_REQUIRED",
      "An acquisition already covers this interest and date. Correct its current approved edition or finish the pending draft first.",
    );
  const net =
    decimalToUnits(input.identifiableNetAssets) -
    decimalToUnits(input.deferredTax);
  if (net < BigInt(0))
    throw new ApiError(
      422,
      "ACQUISITION_NET_ASSETS_INVALID",
      "Deferred tax cannot exceed identifiable net assets in this acquisition policy.",
    );
  const nci =
    input.nciMeasurement === "fair_value"
      ? decimalToUnits(input.nciFairValue)
      : halfAway(
          net * BigInt(10000 - Number(ownership.ownership_bps)),
          BigInt(10000),
        );
  const calculation = {
      consideration: input.consideration,
      previousInterest: input.previousInterest,
      fairValueNetAssets: unitsToDecimal(net),
      nciFairValue: unitsToDecimal(nci),
      parentBps: Number(ownership.ownership_bps),
      openingNci: "0",
      subsidiaryProfit: "0",
      subsidiaryOci: "0",
      subsidiaryDividends: "0",
      nciOwnershipChange: "0",
      associateOpening: "0",
      associateBps: 0,
      associateProfit: "0",
      associateOci: "0",
      associateDividends: "0",
      associateImpairment: "0",
    },
    result = calculateFinance("consolidation", calculation, 2),
    evidence = {
      ownership,
      calculation,
      result,
      nci: unitsToDecimal(nci),
      ...(parent ? { parent } : {}),
    },
    hash = await sha256(JSON.stringify({ input, evidence }));
  return { evidence, hash, parent };
}
export async function prepareAcquisitionEdition(
  actor: WorkContext,
  raw: z.input<typeof acquisitionEditionSchema>,
  key: string,
) {
  actor = actorGuard(actor, "finance.write");
  const input = acquisitionEditionSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    id = `acquisition-${key}`,
    calcId = `acquisition-calc-${key}`,
    op = "finance.acquisition.prepare";
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    await requireFinanceTool(db, actor.companyId, "group_reporting");
    const e = await acquisitionSource(db, actor, input),
      now = Date.now(),
      calcHash = await sha256(
        JSON.stringify({
          kind: "consolidation",
          input: e.evidence.calculation,
          policyId: null,
          policyVersion: 1,
          currency: input.currency,
          scale: 2,
        }),
      );
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      1,
      {
        id,
        status: "draft",
        version: 1,
        calculationRunId: calcId,
        result: e.evidence.result,
      },
      [
        db
          .prepare(
            "INSERT INTO finance_calculation_runs(id,company_id,holding_id,kind,policy_version,currency,currency_scale,source_hash,input_json,result_json,explanation_json,status,prepared_by,created_at,updated_at) VALUES(?,?,?,'consolidation',1,?,2,?,?,?,?,'prepared',?,?,?)",
          )
          .bind(
            calcId,
            actor.companyId,
            actor.holdingId,
            input.currency,
            calcHash,
            JSON.stringify(e.evidence.calculation),
            JSON.stringify(e.evidence.result),
            JSON.stringify({
              engine: "source-acquisition-v1",
              acquisitionId: id,
              sourceHash: e.hash,
            }),
            actor.userId,
            now,
            now,
          ),
        db
          .prepare(
            "INSERT INTO finance_calculation_events VALUES(?,?,1,'prepare',?,?,?,?)",
          )
          .bind(
            crypto.randomUUID(),
            calcId,
            actor.userId,
            input.reason,
            await sha256(JSON.stringify(e.evidence.result)),
            now,
          ),
        db
          .prepare(
            "INSERT INTO finance_acquisitions(id,holding_id,company_id,ownership_id,investee_entity_id,acquisition_date,currency,consideration_minor,previous_interest_minor,nci_measurement,nci_minor,identifiable_net_assets_minor,deferred_tax_minor,goodwill_minor,bargain_purchase_minor,cgu_reference,calculation_run_id,created_by,created_at,updated_at,source_hash,source_json,reason,parent_id,parent_version) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            id,
            actor.holdingId,
            actor.companyId,
            input.ownershipId,
            e.evidence.ownership.investee_entity_id,
            input.acquisitionDate,
            input.currency,
            String(decimalToUnits(input.consideration)),
            String(decimalToUnits(input.previousInterest)),
            input.nciMeasurement,
            String(decimalToUnits(e.evidence.nci)),
            String(decimalToUnits(input.identifiableNetAssets)),
            String(decimalToUnits(input.deferredTax)),
            String(decimalToUnits(String(e.evidence.result.goodwill))),
            String(decimalToUnits(String(e.evidence.result.bargainPurchase))),
            input.cguReference,
            calcId,
            actor.userId,
            now,
            now,
            e.hash,
            JSON.stringify({ input, evidence: e.evidence }),
            input.reason,
            input.parentId ?? null,
            input.parentVersion ?? null,
          ),
      ],
    );
  });
}
export async function decideAcquisitionEdition(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof groupEvidenceDecisionSchema>,
  action: "approve" | "reject",
  key: string,
) {
  actor = actorGuard(actor, "finance.approve");
  const input = groupEvidenceDecisionSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    op = `finance.acquisition.${action}`;
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    const r = await get(db, actor, "finance_acquisitions", id);
    if (
      r.status !== "draft" ||
      r.version !== input.expectedVersion ||
      !r.source_json
    )
      throw new ApiError(
        409,
        "ACQUISITION_CHANGED",
        "Review the current acquisition draft.",
      );
    if (samePerson(actor, r.created_by))
      throw new ApiError(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Finance approver must review acquisition evidence.",
      );
    const definition = acquisitionEditionSchema.parse(
        JSON.parse(String(r.source_json)).input,
      ),
      e =
        action === "approve"
          ? await acquisitionSource(db, actor, definition, id)
          : null;
    if (e && e.hash !== r.source_hash)
      throw new ApiError(
        409,
        "ACQUISITION_SOURCE_CHANGED",
        "Ownership or calculation evidence changed. Reject and prepare again.",
      );
    if (action === "approve") {
      const calc = await db
        .prepare(
          "SELECT * FROM finance_calculation_runs WHERE id=? AND company_id=? AND holding_id=? AND status='prepared'",
        )
        .bind(r.calculation_run_id, actor.companyId, actor.holdingId)
        .first<Row>();
      const expected = JSON.parse(String(r.source_json)).evidence;
      if (
        !calc ||
        calc.input_json !== JSON.stringify(expected.calculation) ||
        calc.result_json !== JSON.stringify(expected.result) ||
        JSON.parse(String(calc.explanation_json)).acquisitionId !== id
      )
        throw new ApiError(
          409,
          "ACQUISITION_CALCULATION_CHANGED",
          "The owned acquisition calculation changed before its independent review.",
        );
    }
    const now = Date.now(),
      status = action === "approve" ? "approved" : "rejected";
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      input.expectedVersion + 1,
      { id, status, version: input.expectedVersion + 1 },
      [
        ...(action === "approve" && e?.parent
          ? [
              assertion(
                db,
                key,
                "acquisition-parent",
                "EXISTS(SELECT 1 FROM finance_acquisitions WHERE id=? AND company_id=? AND holding_id=? AND status='approved' AND version=?)",
                [
                  e.parent.id,
                  actor.companyId,
                  actor.holdingId,
                  Number(definition.parentVersion),
                ],
              ),
              db
                .prepare(
                  "UPDATE finance_acquisitions SET status='superseded',version=version+1,updated_at=? WHERE id=?",
                )
                .bind(now, e.parent.id),
            ]
          : []),
        assertion(
          db,
          key,
          "acquisition-version",
          "EXISTS(SELECT 1 FROM finance_acquisitions WHERE id=? AND company_id=? AND holding_id=? AND status='draft' AND version=?)",
          [id, actor.companyId, actor.holdingId, input.expectedVersion],
        ),
        db
          .prepare(
            "UPDATE finance_acquisitions SET status=?,approved_by=?,version=version+1,updated_at=? WHERE id=?",
          )
          .bind(status, actor.userId, now, id),
        db
          .prepare(
            "UPDATE finance_calculation_runs SET status=?,reviewed_by=?,approved_by=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND status='prepared'",
          )
          .bind(
            status,
            actor.userId,
            actor.userId,
            now,
            r.calculation_run_id,
            actor.companyId,
          ),
        db
          .prepare(
            "INSERT INTO finance_calculation_events VALUES(?,?,2,?,?,?,?,?)",
          )
          .bind(
            crypto.randomUUID(),
            r.calculation_run_id,
            action,
            actor.userId,
            input.reason,
            await sha256(
              JSON.stringify(JSON.parse(String(r.source_json)).evidence.result),
            ),
            now,
          ),
      ],
    );
  });
}
export async function groupOwnershipWorkspace(actor: WorkContext) {
  actor = actorGuard(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const [ownership, acquisitions, entities] = await Promise.all([
    write
      .prepare(
        "SELECT o.*,i.company_id investor_company,v.company_id investee_company,i.reporting_name investor_name,v.reporting_name investee_name FROM finance_ownership_interests o JOIN finance_group_entities i ON i.id=o.investor_entity_id JOIN finance_group_entities v ON v.id=o.investee_entity_id WHERE o.holding_id=? AND o.company_id=? ORDER BY o.created_at DESC,o.id LIMIT 200",
      )
      .bind(actor.holdingId, actor.companyId)
      .all<Row>(),
    write
      .prepare(
        "SELECT a.*,e.reporting_name investee_name FROM finance_acquisitions a JOIN finance_group_entities e ON e.id=a.investee_entity_id WHERE a.holding_id=? AND a.company_id=? ORDER BY a.created_at DESC,a.id LIMIT 200",
      )
      .bind(actor.holdingId, actor.companyId)
      .all<Row>(),
    write
      .prepare(
        "SELECT * FROM finance_group_entities WHERE holding_id=? AND status='active' ORDER BY reporting_name,id",
      )
      .bind(actor.holdingId)
      .all<Row>(),
  ]);
  return {
    ownership: ownership.results,
    acquisitions: acquisitions.results,
    entities: entities.results,
  };
}
