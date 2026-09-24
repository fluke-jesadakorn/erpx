import {listSupplierBatches,supplierBatchWorkspace,createSupplierBatch,decideSupplierBatch,exportSupplierBatch,recoverSupplierExport,recordSupplierResult,confirmSupplierInstruction,supplierBatchSchema,supplierDecisionSchema,supplierExportSchema,supplierResultSchema,supplierConfirmSchema} from "@/server/supplier-payments";
import {recoverRefundExport,listRefunds,refundWorkspace,approveRefund,prepareRefundInstruction,exportRefundInstructions,confirmRefundInstruction,recordRefundException,refundApprovalSchema,refundInstructionSchema,refundExportSchema,refundConfirmationSchema,refundExceptionSchema} from "@/server/refunds";
import { requirePermission } from "@/server/iam-permissions";
import { requestPublicOrigin } from "@/server/public-origin";
import { requireWorkContext } from "@/server/authorization";
import { ApiError, errorResponse } from "@/server/http";
import {
  aging,
  beginClose,
  closePeriod,
  createAccount,
  createAccountSchema,
  changeAccountStatus,
  changeAccountStatusSchema,
  createChart,
  createChartSchema,
  createInvoice,voidDraftInvoice,
  createJournal,
  createJournalSchema,
  createParty,
  createPeriod,
  createPeriodSchema,
  createPayment,
  approvePayment,
  releasePayment,
  confirmPayment,
  confirmPaymentSchema,
  reversePayment,
  reversePaymentSchema,
  allocatePayment,
  exportPaymentFile,
  createTaxCode,
  taxCodeSchema,
  createAsset,
  assetSchema,
  depreciateAsset,
  createBudget,
  budgetSchema,
  createLease,
  leaseSchema,
  createLoan,
  loanSchema,
  createRevenueContract,
  revenueContractSchema,
  createRevenueObligation,
  revenueObligationSchema,
  recognizeRevenue,
  importOpeningTrialBalance,
  openingTrialBalanceSchema,
  previewOpeningTrialBalance,
  importAccounts,
  importBankStatement,
  listAccounts,
  listCharts,
  listJournals,
  listInvoices,
  listPayments,
  getInvoice,
  getPayment,
  postInvoice,
  postJournal,
  reconcileBankTransaction,
  reopenPeriod,
  reverseJournal,
  statements,
  trialBalance,
  partySchema,
  invoiceSchema,
  closeProofSchema,
  prepareFxRevaluation,
  prepareFxRevaluationSchema,
  decideFxRevaluation,
  decideFxRevaluationSchema,
} from "@/server/finance";
import { z } from "zod";
import { calculationDecisionSchema, calculationRunDetail, calculationRunSchema, createCalculationRun, decideCalculationRun, financeEngineWorkspace, snapshotCloseChecks } from "@/server/finance-engines";
import { consolidationDecisionSchema, consolidationEliminationDecisionSchema, consolidationEliminationSchema, consolidationRunSchema, consolidationRunWorkspace, createConsolidationElimination, createConsolidationRun, decideConsolidationElimination, decideConsolidationRun } from "@/server/finance-consolidation";
import { accrualRunPostSchema, accrualScheduleDecisionSchema, accrualSchedulePage, accrualScheduleSchema, createAccrualSchedule, decideAccrualSchedule, postAccrualRun, prepareAccrualRun } from "@/server/finance-accruals";
import { createGroupEntity, createIntercompany, decideIntercompany, groupEntitySchema, intercompanyDecisionSchema, intercompanyTransactionSchema, intercompanyWorkspace } from "@/server/finance-intercompany";
import { createProjectCostAllocation, decideProjectCostAllocation, listProjectCostAllocations, listProjectCostSources, projectCostAllocationDecisionSchema, projectCostAllocationSchema } from "@/server/project-actuals";

function command(request: Request) {
  return request.headers.get("Idempotency-Key");
}

async function body(request: Request) {
  try {
    return await request.json();
  } catch {
    throw new ApiError(422, "INVALID_JSON", "Request body must be valid JSON.");
  }
}

function needCommand(request: Request) {
  const id = command(request);
  if (!id) throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.");
  return id;
}

function segments(path: string[]) {
  return path.filter(Boolean).map((part) => decodeURIComponent(part));
}

function csvRows(csv: string) {
  const rows = csv.trim().split(/\r?\n/).filter(Boolean).map((row) => row.split(",").map((cell) => cell.trim()));
  if (rows.length < 2) throw new ApiError(422, "EMPTY_IMPORT", "CSV must include a header and at least one row.");
  const header = rows[0].map((cell) => cell.toLowerCase());
  return rows.slice(1).map((row) => Object.fromEntries(header.map((key, index) => [key, row[index] ?? ""])));
}

export const dynamic = "force-dynamic";

type FinanceRouteContext = { params: Promise<{ holdingSlug: string; path: string[] }> };

export async function GET(request: Request, context: FinanceRouteContext) {
  try {
    const { holdingSlug, path } = await context.params;
    const actor = requirePermission(await requireWorkContext(holdingSlug),"finance.read");
    const parts = segments(path); const search = new URL(request.url).searchParams;
    if (parts[0] === "charts") {
      if (parts[1] && parts[2] === "accounts") return Response.json({ data: await listAccounts(actor, parts[1]) });
      return Response.json({ data: await listCharts(actor) });
    }
    if(parts[0]==="supplier-batches"&&parts.length===3&&parts[2]==="export")return Response.json({data:await recoverSupplierExport(actor,parts[1])});
    if(parts[0]==="supplier-batches")return Response.json({data:parts[1]?await supplierBatchWorkspace(actor,parts[1]):await listSupplierBatches(actor)});
    if(parts[0]==="refund-instructions"&&parts.length===3&&parts[2]==="export")return Response.json({data:await recoverRefundExport(actor,parts[1])});
    if(parts[0]==="refunds")return Response.json({data:parts[1]?await refundWorkspace(actor,parts[1]):await listRefunds(actor)});
    if (parts[0] === "periods") return Response.json({ data: await listPeriods(actor) });
    if (parts[0] === "journals") return Response.json({ data: await listJournals(actor, { periodId: search.get("periodId") ?? undefined, status: search.get("status") ?? undefined }) });
    if (parts[0] === "project-cost-allocations") {
      const projectId = search.get("projectId");
      if (!projectId) throw new ApiError(422, "PROJECT_ID_REQUIRED", "Select a project for project cost allocations.");
      return Response.json({ data: await listProjectCostAllocations(actor, projectId) });
    }
    if (parts[0] === "project-cost-sources") {
      const projectId = search.get("projectId");
      if (!projectId) throw new ApiError(422, "PROJECT_ID_REQUIRED", "Select a project for posted cost sources.");
      return Response.json({ data: await listProjectCostSources(actor, projectId) });
    }
    if (parts[0] === "invoices") return parts[1] ? Response.json({ data: await getInvoice(actor, parts[1]) }) : Response.json({ data: await listInvoices(actor) });
    if (parts[0] === "payments") return parts[1] ? Response.json({ data: await getPayment(actor, parts[1]) }) : Response.json({ data: await listPayments(actor) });
    if (parts[0] === "trial-balance") return Response.json({ data: await trialBalance(actor, search.get("periodId") ?? undefined,search.get("chartId")??undefined) });
    if (parts[0] === "statements") return Response.json({ data: await statements(actor, search.get("periodId") ?? undefined,search.get("chartId")??undefined) });
    if (parts[0] === "aging") { const kind = search.get("kind"); if (kind !== "payable" && kind !== "receivable") throw new ApiError(422, "INVALID_AGING_KIND", "kind must be payable or receivable."); return Response.json({ data: await aging(actor, kind, search.get("asOf") ?? new Date().toISOString().slice(0, 10)) }); }
    if(parts[0]==="engines")return Response.json({data:await financeEngineWorkspace(actor)});
    if(parts[0]==="intercompany")return Response.json({data:await intercompanyWorkspace(actor)});
    if(parts[0]==="group-entities")return Response.json({data:await intercompanyWorkspace(actor)});
    if(parts[0]==="consolidation-runs")return Response.json({data:await consolidationRunWorkspace(actor)});
    if(parts[0]==="accrual-schedules")return Response.json({data:await accrualSchedulePage(actor)});
    if(parts[0]==="calculation-runs"&&parts[1])return Response.json({data:await calculationRunDetail(actor,parts[1])});
    throw new ApiError(404, "NOT_FOUND", "Finance resource not found.");
  } catch (error) { return errorResponse(error); }
}

export async function POST(request: Request, context: FinanceRouteContext) {
  try {
    const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const { holdingSlug, path } = await context.params;
    const actor = requirePermission(await requireWorkContext(holdingSlug),"finance.read");
    const parts = segments(path); const input = await body(request);
    if(parts[0]==="project-cost-allocations"&&parts.length===1){const parsed=projectCostAllocationSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Invalid project cost allocation.");return Response.json({data:await createProjectCostAllocation(actor,parsed.data,needCommand(request))},{status:201});}
    if(parts[0]==="project-cost-allocations"&&parts.length===3&&parts[2]==="review"){const parsed=projectCostAllocationDecisionSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Project cost review evidence is required.");return Response.json({data:await decideProjectCostAllocation(actor,parts[1],parsed.data,needCommand(request))});}
    if(parts[0]==="calculation-runs"&&parts.length===1)return Response.json({data:await createCalculationRun(actor,calculationRunSchema.parse(input),needCommand(request))},{status:201});
    if(parts[0]==="calculation-runs"&&parts.length===3&&["review","approve","reject"].includes(parts[2]))return Response.json({data:await decideCalculationRun(actor,parts[1],calculationDecisionSchema.parse(input),parts[2] as "review"|"approve"|"reject",needCommand(request))});
    if(parts[0]==="intercompany"&&parts.length===1){const parsed=intercompanyTransactionSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Invalid intercompany transaction.");return Response.json({data:await createIntercompany(actor,parsed.data,needCommand(request))},{status:201});}
    if(parts[0]==="intercompany"&&parts.length===3&&["match","settle","eliminate"].includes(parts[2])){const parsed=intercompanyDecisionSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Intercompany decision evidence is required.");return Response.json({data:await decideIntercompany(actor,parts[1],parts[2] as "match"|"settle"|"eliminate",parsed.data,needCommand(request))});}
    if(parts[0]==="group-entities"&&parts.length===1){const parsed=groupEntitySchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Group entity details are invalid.");return Response.json({data:await createGroupEntity(actor,parsed.data,needCommand(request))},{status:201});}
    if(parts[0]==="consolidation-runs"&&parts.length===3&&parts[2]==="eliminations"){const parsed=consolidationEliminationSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Consolidation elimination details are invalid.");return Response.json({data:await createConsolidationElimination(actor,parts[1],parsed.data,needCommand(request))},{status:201});}
    if(parts[0]==="consolidation-eliminations"&&parts.length===3&&["approve","post"].includes(parts[2])){const parsed=consolidationEliminationDecisionSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Consolidation elimination decision evidence is required.");return Response.json({data:await decideConsolidationElimination(actor,parts[1],parts[2] as "approve"|"post",parsed.data,needCommand(request))});}
    if(parts[0]==="consolidation-runs"&&parts.length===1){const parsed=consolidationRunSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Consolidation run details are invalid.");return Response.json({data:await createConsolidationRun(actor,parsed.data,needCommand(request))},{status:201});}
    if(parts[0]==="consolidation-runs"&&parts.length===3&&["review","approve","publish"].includes(parts[2])){const parsed=consolidationDecisionSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Consolidation decision evidence is required.");return Response.json({data:await decideConsolidationRun(actor,parts[1],parts[2] as "review"|"approve"|"publish",parsed.data,needCommand(request))});}
    if(parts[0]==="accrual-schedules"&&parts.length===1)return Response.json({data:await createAccrualSchedule(actor,accrualScheduleSchema.parse(input),needCommand(request))},{status:201});
    if(parts[0]==="accrual-schedules"&&parts.length===3&&["approve","cancel"].includes(parts[2]))return Response.json({data:await decideAccrualSchedule(actor,parts[1],accrualScheduleDecisionSchema.parse(input),parts[2] as "approve"|"cancel",needCommand(request))});
    if(parts[0]==="accrual-schedules"&&parts.length===3&&parts[2]==="prepare")return Response.json({data:await prepareAccrualRun(actor,parts[1],needCommand(request))},{status:201});
    if(parts[0]==="accrual-runs"&&parts.length===3&&parts[2]==="post")return Response.json({data:await postAccrualRun(actor,parts[1],accrualRunPostSchema.parse(input),needCommand(request))});
    if(parts[0]==="fx-revaluations"&&parts.length===1)return Response.json({data:await prepareFxRevaluation(actor,prepareFxRevaluationSchema.parse(input),needCommand(request))},{status:201});
    if(parts[0]==="fx-revaluations"&&parts.length===3&&(parts[2]==="approve"||parts[2]==="reject"))return Response.json({data:await decideFxRevaluation(actor,parts[1],decideFxRevaluationSchema.parse(input),parts[2],needCommand(request))});
    if(parts[0]==="periods"&&parts.length===3&&parts[2]==="close-checks")return Response.json({data:await snapshotCloseChecks(actor,parts[1],needCommand(request))});
    if(parts[0]==="supplier-batches"&&parts.length===1)return Response.json({data:await createSupplierBatch(actor,supplierBatchSchema.parse(input),needCommand(request))},{status:201});
    if(parts[0]==="supplier-batches"&&parts.length===3&&(parts[2]==="approve"||parts[2]==="cancel"))return Response.json({data:await decideSupplierBatch(actor,parts[1],supplierDecisionSchema.parse(input),needCommand(request),parts[2])});
    if(parts[0]==="supplier-batches"&&parts.length===3&&parts[2]==="export")return Response.json({data:await exportSupplierBatch(actor,parts[1],supplierExportSchema.parse(input),needCommand(request))});
    if(parts[0]==="supplier-instructions"&&parts.length===3&&parts[2]==="result")return Response.json({data:await recordSupplierResult(actor,parts[1],supplierResultSchema.parse(input),needCommand(request))});
    if(parts[0]==="supplier-instructions"&&parts.length===3&&parts[2]==="confirm")return Response.json({data:await confirmSupplierInstruction(actor,parts[1],supplierConfirmSchema.parse(input),needCommand(request))});
    if(parts[0]==="refunds"&&parts.length===3&&parts[2]==="approve")return Response.json({data:await approveRefund(actor,parts[1],refundApprovalSchema.parse(input),needCommand(request))});
    if(parts[0]==="refunds"&&parts.length===3&&parts[2]==="instructions")return Response.json({data:await prepareRefundInstruction(actor,parts[1],refundInstructionSchema.parse(input),needCommand(request))},{status:201});
    if(parts[0]==="refund-instructions"&&parts.length===2&&parts[1]==="export")return Response.json({data:await exportRefundInstructions(actor,refundExportSchema.parse(input),needCommand(request))});
    if(parts[0]==="refund-instructions"&&parts.length===3&&parts[2]==="confirm")return Response.json({data:await confirmRefundInstruction(actor,parts[1],refundConfirmationSchema.parse(input),needCommand(request))});
    if(parts[0]==="refund-instructions"&&parts.length===3&&parts[2]==="exception")return Response.json({data:await recordRefundException(actor,parts[1],refundExceptionSchema.parse(input),needCommand(request))});
    if (parts[0] === "charts" && parts[1] === "import") {
      const normalized = typeof input === "object" && input !== null && "csv" in input && typeof input.csv === "string" ? { ...(input as Record<string, unknown>), rows: csvRows(input.csv) } : input;
      const parsed = z.object({ chartId: z.string().min(1), rows: z.array(z.object({ code: z.string(), name: z.string(), category: z.enum(["asset", "liability", "equity", "revenue", "expense", "other_income", "other_expense"]), currency: z.string().optional(), controlKind:z.enum(["none","receivable","payable"]).optional() })).min(1) }).safeParse(normalized);
      if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid chart import.");
      return Response.json({ data: await importAccounts(actor, parsed.data.chartId, parsed.data.rows, needCommand(request)) }, { status: 201 });
    }
    if (parts[0] === "charts" && parts.length === 1) { const parsed = createChartSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid chart."); return Response.json({ data: await createChart(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "charts" && parts[2] === "accounts" && parts[4] === "status" && parts.length === 5) { const parsed = changeAccountStatusSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid account status change."); return Response.json({ data: await changeAccountStatus(actor, parts[1], parts[3], parsed.data, needCommand(request)) }); }
    if (parts[0] === "charts" && parts[2] === "accounts" && parts.length === 3) { const parsed = createAccountSchema.safeParse({ ...(typeof input === "object" && input !== null ? input : {}), chartId: parts[1] }); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid account."); return Response.json({ data: await createAccount(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "periods" && parts.length === 1) { const parsed = createPeriodSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid period."); return Response.json({ data: await createPeriod(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "journals" && parts.length === 1) { const parsed = createJournalSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid journal."); return Response.json({ data: await createJournal(actor, {...parsed.data,sourceType:"manual",sourceId:undefined,isOpening:false}, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "opening-balances" && parts[1] === "preview") { const parsed = openingTrialBalanceSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Opening trial balance is invalid."); return Response.json({ data: await previewOpeningTrialBalance(actor, parsed.data) }); }
    if (parts[0] === "opening-balances" && parts[1] === "import") { const parsed = openingTrialBalanceSchema.extend({previewToken:z.string().length(64)}).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Opening trial balance is invalid."); return Response.json({ data: await importOpeningTrialBalance(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "journals" && parts[2] === "post") { const parsed = z.object({ expectedVersion: z.number().int().positive() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "expectedVersion is required."); return Response.json({ data: await postJournal(actor, parts[1], needCommand(request), parsed.data) }); }
    if (parts[0] === "journals" && parts[2] === "reverse") { const parsed = z.object({ periodId: z.string(), entryDate: z.string().date(), memo: z.string().optional() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "A reversal period and date are required."); return Response.json({ data: await reverseJournal(actor, parts[1], needCommand(request), parsed.data) }, { status: 201 }); }
    if (parts[0] === "periods" && parts[2] === "begin-close") return Response.json({ data: await beginClose(actor, parts[1], needCommand(request)) });
    if (parts[0] === "periods" && parts[2] === "close") { const parsed = closeProofSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Close evidence is required."); return Response.json({ data: await closePeriod(actor, parts[1], parsed.data, needCommand(request)) }); }
    if (parts[0] === "periods" && parts[2] === "reopen") { const parsed = closeProofSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Reopen evidence is required."); return Response.json({ data: await reopenPeriod(actor, parts[1], parsed.data, needCommand(request)) }); }
    if (parts[0] === "parties" && parts.length === 1) { const parsed = partySchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid party."); return Response.json({ data: await createParty(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "invoices" && parts.length === 1) { const parsed = invoiceSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid invoice."); return Response.json({ data: await createInvoice(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if(parts[0]==="invoices"&&parts.length===2){const parsed=invoiceSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0].message);return Response.json({data:await createInvoice(actor,parsed.data,needCommand(request),parts[1])});}
    if(parts[0]==="invoices"&&parts[2]==="void"){const data=z.object({expectedVersion:z.number().int().positive()}).parse(input);return Response.json({data:await voidDraftInvoice(actor,parts[1],data.expectedVersion,needCommand(request))});}
    if (parts[0] === "invoices" && parts[2] === "post") { const parsed = z.object({ expectedVersion:z.number().int().positive(), periodId: z.string(), entryDate: z.string().date(), fxRate:z.string().optional() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Posting period, date and a valid exchange rate are required."); return Response.json({ data: await postInvoice(actor, parts[1], parsed.data, needCommand(request)) }); }
    if (parts[0] === "payments" && parts.length === 1) { const parsed = z.object({ partyId: z.string().optional(), kind: z.enum(["payment", "receipt", "refund"]), paymentNumber: z.string(), paymentDate: z.string().date(), currency: z.string().length(3), amount: z.string(), bankAccount: z.string().optional() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid payment."); return Response.json({ data: await createPayment(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "payments" && parts[2] === "approve") { const parsed = z.object({ expectedVersion: z.number().int().positive(), proof: z.string() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Payment approval evidence is required."); return Response.json({ data: await approvePayment(actor, parts[1], parsed.data, needCommand(request)) }); }
    if (parts[0] === "payments" && parts[2] === "release") { const parsed = z.object({ expectedVersion: z.number().int().positive(), proof: z.string() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Payment release evidence is required."); return Response.json({ data: await releasePayment(actor, parts[1], parsed.data, needCommand(request)) }); }
    if (parts[0] === "payments" && parts[2] === "confirm") { const parsed = confirmPaymentSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message??"Payment confirmation is invalid."); return Response.json({ data: await confirmPayment(actor, parts[1], parsed.data, needCommand(request)) }); }
    if (parts[0] === "payments" && parts[2] === "reverse") { const parsed = reversePaymentSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message??"Payment correction evidence is required."); return Response.json({ data: await reversePayment(actor, parts[1], parsed.data, needCommand(request)) }); }
    if (parts[0] === "payments" && parts[2] === "allocate") { const parsed = z.object({ invoiceId: z.string(), amount: z.string(), expectedVersion: z.number().int().positive() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Payment allocation is invalid."); return Response.json({ data: await allocatePayment(actor, parts[1], parsed.data, needCommand(request)) }); }
    if (parts[0] === "payments" && parts[1] === "export") { const parsed = z.object({ paymentIds: z.array(z.string()).min(1) }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Select released payments."); return Response.json({ data: await exportPaymentFile(actor, parsed.data.paymentIds, needCommand(request)) }); }
    if (parts[0] === "tax-codes" && parts.length === 1) { const parsed = taxCodeSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid tax code."); return Response.json({ data: await createTaxCode(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "assets" && parts.length === 1) { const parsed = assetSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid asset."); return Response.json({ data: await createAsset(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "assets" && parts[2] === "depreciate") { const parsed = z.object({ periodId: z.string(), entryDate: z.string().date() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Depreciation period and date are required."); return Response.json({ data: await depreciateAsset(actor, parts[1], parsed.data, needCommand(request)) }); }
    if (parts[0] === "budgets" && parts.length === 1) { const parsed = budgetSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid budget."); return Response.json({ data: await createBudget(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "expenses") throw new ApiError(410,"EXPENSE_API_MOVED","Use the /expenses workflow for employee obligations.");
    if (parts[0] === "leases" && parts.length === 1) { const parsed = leaseSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid lease."); return Response.json({ data: await createLease(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "loans" && parts.length === 1) { const parsed = loanSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid loan."); return Response.json({ data: await createLoan(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "revenue-contracts" && parts.length === 1) { const parsed = revenueContractSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid revenue contract."); return Response.json({ data: await createRevenueContract(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "revenue-obligations" && parts.length === 1) { const parsed = revenueObligationSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid revenue obligation."); return Response.json({ data: await createRevenueObligation(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "revenue-obligations" && parts[2] === "recognize") { const parsed = z.object({ amount: z.string(), periodId: z.string(), entryDate: z.string().date() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Revenue recognition period, date and amount are required."); return Response.json({ data: await recognizeRevenue(actor, parts[1], parsed.data, needCommand(request)) }); }
    if (parts[0] === "bank" && parts[1] === "statements" && parts[2] === "import") { const parsed = z.object({ bankAccount: z.string(), statementNumber: z.string(), statementDate: z.string().date(), opening: z.string(), closing: z.string(), closingFxRate:z.string().optional(), sourceHash: z.string().min(8), rows: z.array(z.object({ externalId: z.string(), transactionDate: z.string().date(), description: z.string().optional(), amount: z.string(), currency: z.string().length(3) })).min(1) }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid bank statement."); return Response.json({ data: await importBankStatement(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "bank" && parts[1] === "transactions" && parts[3] === "reconcile") { const parsed = z.object({ expectedVersion: z.number().int().positive(), matchedType: z.string(), matchedId: z.string(), proof: z.string() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Reconciliation evidence is required."); return Response.json({ data: await reconcileBankTransaction(actor, parts[2], parsed.data, needCommand(request)) }); }
    throw new ApiError(404, "NOT_FOUND", "Finance resource not found.");
  } catch (error) { return errorResponse(error); }
}

async function listPeriods(actor: import("@/server/authorization").WorkContext) {
  const { holdingResources } = await import("@/server/runtime");
  const { write } = await holdingResources(actor.holdingId);
  return (await write.prepare("SELECT * FROM finance_periods WHERE company_id=? ORDER BY start_date DESC").bind(actor.companyId).all()).results;
}
