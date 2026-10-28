import { nativeJournalCreate, nativeJournalCreateReadback, nativeJournalPost, nativeJournalPostReadback, nativeJournalReverse, nativeJournalReverseReadback } from "@erpx/feature-finance/public/server";
import {expensePostingPrerequisites,createExpensePostingChart,createExpensePostingAccount,createExpensePostingPeriod,confirmExpensePostingRate,expensePostingPrerequisiteConfirmation,expensePostingChartSchema,expensePostingAccountSchema,expensePostingPeriodSchema,expensePostingRateSchema,expensePostingPrerequisiteConfirmationSchema} from '@erpx/server-finance/public/expense-posting-prerequisites';
import {expensePostingSetup,initializeExpensePostingSetup,expensePostingSetupSchema} from '@erpx/server-commerce/public/expense-posting-setup';
import {confirmServiceNoCharge} from "@erpx/server-finance/public/service-no-charge";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {editManualDraft,manualDraftEditSchema} from "@erpx/server-finance/public/finance-manual-drafts";
import {financeEntrypointQueries} from "@erpx/server-finance/public/entrypoint-queries";
import {prepareBillingCorrection,decideBillingCorrection,billingCorrectionSchema,billingCorrectionDecisionSchema} from "@erpx/server-finance/public/finance-billing-corrections";
import {instrumentWorkspace,prepareInstrument,decideInstrument,prepareInstrumentRun,decideInstrumentRun,reverseInstrumentRun,instrumentEditionSchema,instrumentDecisionSchema,instrumentRunSchema,instrumentReversalSchema} from "@erpx/server-finance/public/finance-instruments";
import {assetPolicyWorkspace,prepareAssetPolicy,decideAssetPolicy,prepareAssetPolicyRun,decideAssetPolicyRun,reverseAssetPolicyRun,assetPolicySchema,assetPolicyDecisionSchema,assetPolicyRunSchema,assetPolicyReversalSchema} from "@erpx/server-finance/public/finance-asset-policies";
import { financeAiWorkspace, saveFinanceAiPolicy, queueFinanceAi, dismissFinanceAi, applyFinanceAiDraft } from "@erpx/server-finance/public/finance-ai";
import { financeAiDecisionSchema } from "@erpx/feature-finance/lib/finance-ai";
import { holdingResources, runtimeEnv } from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import { accountingProfiles,prepareAccountingProfile,decideAccountingProfile,accountingProfileSchema,accountingProfileDecisionSchema,financialEvents } from "@erpx/server-finance/public/accounting-profiles";
import { vatWorkspace,vatPackageDetail,vatPackageExport,prepareVatEntry,reviewVatEntry,prepareVatCorrection,reviewVatCorrection,vatCorrectionDetail,taxEntryDetail,vatCorrectionSchema,prepareVatPackage,approveVatPackage,exportVatWorkpaper,recordVatFiling,supersedeVatPackage,vatEntrySchema,taxReviewSchema,vatPackageSchema,vatFilingSchema } from "@erpx/server-finance/public/finance-tax-register";
import {listSupplierBatches,supplierBatchWorkspace,createSupplierBatch,decideSupplierBatch,exportSupplierBatch,recoverSupplierExport,recordSupplierResult,confirmSupplierInstruction,supplierBatchSchema,supplierDecisionSchema,supplierExportSchema,supplierResultSchema,supplierConfirmSchema} from "@erpx/server-procurement/public/supplier-payments";
import {importRefundBankEvidence,refundBankEvidenceSchema,recoverRefundExport,listRefunds,refundWorkspace,approveRefund,prepareRefundInstruction,exportRefundInstructions,confirmRefundInstruction,recordRefundException,refundApprovalSchema,refundInstructionSchema,refundExportSchema,refundConfirmationSchema,refundExceptionSchema} from "@erpx/server-finance/public/refunds";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import {prepareGroupReportEdition,decideGroupReportEdition,groupReportEditionExport,groupExpansionWorkspace,groupReportEditionSchema,groupReportDecisionSchema} from "@erpx/server-finance/public/finance-group-editions";
import {prepareOwnershipEdition,decideOwnershipEdition,prepareAcquisitionEdition,decideAcquisitionEdition,ownershipEditionSchema,acquisitionEditionSchema,groupEvidenceDecisionSchema} from "@erpx/server-finance/public/finance-group-ownership";
import {prepareIntercompanyPair,decideIntercompanyPair,intercompanyPairSchema,intercompanyPairDecisionSchema} from "@erpx/server-finance/public/finance-intercompany-pairs";
import {bookAdjustmentWorkspace,prepareBookAdjustment,decideBookAdjustment,reverseBookAdjustment,bookAdjustmentSchema,bookAdjustmentReversalSchema} from "@erpx/server-finance/public/finance-book-adjustments";
import { businessContractWorkspace,prepareBusinessContract,decideBusinessContract,prepareBusinessBilling,prepareRecognitionRun,decideRecognitionRun,reverseRecognitionRun,businessContractSchema,businessBillingSchema,businessDecisionSchema,recognitionRunSchema,recognitionReversalSchema } from "@erpx/server-finance/public/finance-business-contracts";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import {createFinanceStarter,financeStarterSchema} from "@erpx/server-finance/public/finance-starter";
import {aging,beginClose,cancelClose,cancelCloseSchema,closePeriod,createAccount,createAccountSchema,changeAccountStatus,changeAccountStatusSchema,createChart,createChartSchema,createInvoice,voidDraftInvoice,createJournal,createJournalSchema,createParty,createPeriod,createPeriodSchema,createPayment,approvePayment,releasePayment,confirmPayment,confirmPaymentSchema,reversePayment,reversePaymentSchema,allocatePayment,exportPaymentFile,createTaxCode,taxCodeSchema,createAsset,assetSchema,depreciateAsset,createBudget,budgetSchema,createLease,leaseSchema,createLoan,loanSchema,createRevenueContract,revenueContractSchema,createRevenueObligation,revenueObligationSchema,recognizeRevenue,importOpeningTrialBalance,openingTrialBalanceSchema,previewOpeningTrialBalance,importAccounts,importBankStatement,listAccounts,listCharts,listJournals,listInvoices,listPayments,getPayment,postInvoice,invoicePostingSchema,postJournal,postJournalBatch,journalBatchSchema,reconcileBankTransaction,reopenPeriod,reverseJournal,statements,trialBalance,reportDateSchema,partySchema,invoiceSchema,closeProofSchema,prepareFxRevaluation,prepareFxRevaluationSchema,decideFxRevaluation,decideFxRevaluationSchema} from '@erpx/server-finance/public/finance';
import {getInvoice} from '@erpx/server-finance/public/finance-invoice-read';
import { z } from "zod";
import {financeDueWorkspace,scheduleFinanceDraft,decideFinanceDueDraft,financeDueDraftSchema,financeDueDecisionSchema} from "@erpx/server-finance/public/finance-due-drafts";
import {decideNoChargeBilling,prepareBusinessBillingRevision,billingRevisionSchema} from "@erpx/server-finance/public/finance-business-contracts";
import { bookRuleSchema,dimensionSchema,financeReviewSchema,financeBookWorkspace,createFinanceDimension,prepareBookRule,decideBookRule } from "@erpx/server-finance/public/finance-books";
import { budgetEditionSchema,forecastEditionSchema,financialReportSchema,prepareBudgetEdition,importBudgetCsv,prepareForecastEdition,prepareFinancialReport,decideFinancePlan,financePlanningWorkspace,budgetActuals,snapshotFinancialReport,exportFinancialReport } from "@erpx/server-finance/public/finance-planning";
import { financeCompanyTools,financeToolSchema,setFinanceCompanyTool } from "@erpx/server-finance/public/finance-company-tools";
import { bankCsvMappingSchema,bankCsvImportSchema,bankMatchGroupSchema,createBankCsvMapping,importMappedBankCsv,proposeBankMatchGroup,decideBankMatchGroup,bankAutomationWorkspace } from "@erpx/server-finance/public/finance-bank-automation";
import { collectionCaseSchema,reviseCollectionCaseSchema,reviseCollectionCase,prepareCollectionCase,decideCollectionCase,collectionWorkspace,cashDueForecast } from "@erpx/server-finance/public/finance-collections";
import { activateAsset,assetActivationSchema,assetDecisionSchema,assetDepreciationSchema,assetDetail,assetDisposalSchema,assetEditSchema,assetPage,cancelDraftAsset,decideAssetDisposal,editAsset,prepareAssetDisposal } from "@erpx/server-finance/public/finance-assets";
import { calculationDecisionSchema, calculationRunDetail, calculationRunSchema, createCalculationRun, decideCalculationRun, financeEngineWorkspace, snapshotCloseChecks } from "@erpx/server-finance/public/finance-engines";
import { consolidationDecisionSchema, consolidationEliminationDecisionSchema, consolidationEliminationSchema, consolidationRunSchema, consolidationRunWorkspace, createConsolidationElimination, createConsolidationRun, decideConsolidationElimination, decideConsolidationRun } from "@erpx/server-finance/public/finance-consolidation";
import { accrualRunPostSchema, accrualScheduleDecisionSchema, accrualSchedulePage, accrualScheduleSchema, createAccrualSchedule, decideAccrualSchedule, postAccrualRun, prepareAccrualRun } from "@erpx/server-finance/public/finance-accruals";
import { createGroupEntity, createIntercompany, decideIntercompany, groupEntitySchema, intercompanyDecisionSchema, intercompanyTransactionSchema, intercompanyWorkspace } from "@erpx/server-finance/public/finance-intercompany";
import { createProjectCostAllocation, decideProjectCostAllocation, listProjectCostAllocations, listProjectCostSources, projectCostAllocationDecisionSchema, projectCostAllocationSchema } from "@erpx/server-projects/public/project-actuals";
import { decideStatementPackage, decideYearClosing, prepareStatementPackage, prepareYearClosing, reportingDecisionSchema, reportingWorkspace, statementPackageDetail, statementPackageExport, statementPackageSchema, yearClosingSchema } from "@erpx/server-finance/public/finance-reporting";
import {decideTaxIdentity,prepareTaxIdentity,taxIdentityDecisionSchema,taxIdentitySchema,taxIdentityWorkspace} from "@erpx/server-finance/public/finance-tax-identities";
import {decideWithholdingPackage,prepareWithholdingPackage,remitWithholdingPackage,reverseWithholdingRemittance,withholdingMonthlyWorkspace,withholdingPackageExport,withholdingPackageSchema,withholdingPackageDecisionSchema,withholdingRemittanceSchema,withholdingRemittanceReversalSchema} from "@erpx/server-finance/public/finance-withholding-remittance";
import {decideWithholdingAssessment,prepareWithholdingAssessment,withholdingAssessmentSchema,withholdingDecisionSchema,withholdingWorkspace} from "@erpx/server-finance/public/finance-withholding";
import {decideThaiDocumentChange,issueThaiTaxInvoice,prepareThaiDocumentChange,thaiDocumentChangeSchema,thaiDocumentDecisionSchema,thaiDocumentDetail,thaiDocumentExport,thaiInvoiceIssueSchema} from "@erpx/server-finance/public/finance-thai-documents";

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
    const parts = segments(path);
    if (parts.length === 3 && parts[0] === "journals" && parts[2] === "create-workspace") {
      const response = await nativeJournalCreateReadback(request, holdingSlug, parts[1]);
      if (response) return response;
      throw new ApiError(404, "NOT_FOUND", "Journal not found.");
    }
    if (parts.length === 3 && parts[0] === "journals" && parts[2] === "reversal-workspace") {
      const response = await nativeJournalReverseReadback(request, holdingSlug, parts[1]);
      if (response) return response;
      throw new ApiError(404, "NOT_FOUND", "Journal not found.");
    }
    if (parts.length === 3 && parts[0] === "journals" && parts[2] === "workspace") {
      const response = await nativeJournalPostReadback(request, holdingSlug, parts[1]);
      if (response) return response;
    }
    const actor = requirePermission(await requireWorkContext(holdingSlug),"finance.read");
    const search = new URL(request.url).searchParams;
    if(parts[0]?.startsWith('expense-setup')){
      if(parts[0]!=='expense-setup'||parts.length!==1)throw new ApiError(404,'NOT_FOUND','Expense setup resource not found.');
      if([...search.keys()].length)throw new ApiError(422,'VALIDATION_FAILED','Expense setup uses the current Company without a query selector.');
      // Separate named snapshots each carry their own opaque source context;
      // this transport does not claim a distributed atomic read across them.
      return Response.json(canonicalDestinationLinks({data:{prerequisites:await expensePostingPrerequisites(actor),mapping:await expensePostingSetup(actor)}}),{headers:{'Cache-Control':'private, no-store'}});
    }
    if(parts[0]==="due-drafts"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await financeDueWorkspace(actor)}));
    if(parts[0]==="group-editions"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await groupExpansionWorkspace(actor)}));
    if(parts[0]==="group-editions"&&parts.length===3&&parts[2]==="export"){const data=await groupReportEditionExport(actor,parts[1]);return new Response(data.content,{headers:{"Content-Type":"text/csv; charset=utf-8","Content-Disposition":`attachment; filename="${data.filename}"`,"X-Content-SHA256":data.sha256}});}
    if(parts[0]==="instruments"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await instrumentWorkspace(actor)}));
    if(parts[0]==="asset-policies"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await assetPolicyWorkspace(actor)}));
    if(parts[0]==="book-adjustments"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await bookAdjustmentWorkspace(actor)}));
    if(parts[0]==="business-contracts"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await businessContractWorkspace(actor)}));
    if(parts[0]==="company-tools"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await financeCompanyTools(actor)}));
    if(parts[0]==="bank-match-groups"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await bankAutomationWorkspace(actor,{bankCursor:search.get("bankCursor")??undefined,cashCursor:search.get("cashCursor")??undefined,groupCursor:search.get("groupCursor")??undefined,chart:search.get("chart")??undefined,q:search.get("q")??undefined,cashQuery:search.get("cashQuery")??undefined})}));
    if(parts[0]==="collection-cases"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await collectionWorkspace(actor,search.get("asOf")??new Date().toISOString().slice(0,10),{invoiceCursor:search.get("invoiceCursor")??undefined,caseCursor:search.get("caseCursor")??undefined,q:search.get("q")??undefined})}));
    if(parts[0]==="cash-forecast"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await cashDueForecast(actor,{chartId:search.get("chartId")??"",asOf:search.get("asOf")??new Date().toISOString().slice(0,10)})}));
    if(parts.length===1&&parts[0]==="book-mappings")return Response.json(canonicalDestinationLinks({data:await financeBookWorkspace(actor)}));
    if(parts.length===1&&parts[0]==="planning")return Response.json(canonicalDestinationLinks({data:await financePlanningWorkspace(actor,search.get("chartId")??"")}));
    if(parts.length===3&&parts[0]==="budget-revisions"&&parts[2]==="actuals")return Response.json(canonicalDestinationLinks({data:await budgetActuals(actor,parts[1])}));
    if(parts.length===3&&parts[0]==="report-snapshots"&&parts[2]==="export"){const exported=await exportFinancialReport(actor,parts[1]);return new Response(exported.content,{headers:{"Content-Type":"text/csv; charset=utf-8","Content-Disposition":'attachment; filename="financial-report.csv"',"Cache-Control":"private, no-store","X-Content-SHA256":exported.sha256}});}
    if((parts[0]==="statement-packages"||parts[0]==="year-closings")&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await reportingWorkspace(actor,search.get("chartId")??"")}));
    if(parts[0]==="statement-packages"&&parts.length===2)return Response.json(canonicalDestinationLinks({data:await statementPackageDetail(actor,parts[1])}));
    if(parts[0]==="statement-packages"&&parts.length===3&&parts[2]==="export"){
      const exported=await statementPackageExport(actor,parts[1]);
      return new Response(exported.content,{headers:{"Content-Type":"text/html; charset=utf-8","Content-Disposition":`attachment; filename="financial-statements-edition-${exported.edition}.html"`,"Cache-Control":"private, no-store","X-Content-Type-Options":"nosniff","X-Content-SHA256":exported.sha256}});
    }
    if(parts[0]==="assets"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await assetPage(actor)}));
    if(parts[0]==="assets"&&parts.length===2)return Response.json(canonicalDestinationLinks({data:await assetDetail(actor,parts[1])}));
    if(parts[0]==="ai"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await financeAiWorkspace((await holdingResources(actor.holdingId)).write,actor,await runtimeEnv())}));
    if(parts[0]==="accounting-profiles")return Response.json(canonicalDestinationLinks({data:await accountingProfiles(actor)}));
    if(parts[0]==="financial-events")return Response.json(canonicalDestinationLinks({data:await financialEvents(actor,{chartId:search.get("chartId")??"",startDate:search.get("startDate")??undefined,endDate:search.get("endDate")??undefined})}));
    if(parts[0]==="tax-identities"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await taxIdentityWorkspace(actor)}));
    if(parts[0]==="withholding-months"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await withholdingMonthlyWorkspace(actor)}));
    if(parts[0]==="withholding-months"&&parts.length===3&&parts[2]==="export"){
      const exported=await withholdingPackageExport(actor,parts[1]);return new Response(exported.content,{headers:{"Content-Type":"text/csv; charset=utf-8","Content-Disposition":`attachment; filename="${exported.formKind}-${exported.month}-manual-workpaper.csv"`,"Cache-Control":"private, no-store","X-Content-Type-Options":"nosniff","X-Content-SHA256":exported.sha256}});
    }
    if(parts[0]==="withholding"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await withholdingWorkspace(actor,search.get("paymentId")??undefined)}));
    if(parts[0]==="thai-documents"&&parts.length===2)return Response.json(canonicalDestinationLinks({data:await thaiDocumentDetail(actor,parts[1])}));
    if(parts[0]==="thai-documents"&&parts.length===3&&parts[2]==="export"){
      const exported=await thaiDocumentExport(actor,parts[1]);return new Response(exported.content,{headers:{"Content-Type":"text/html; charset=utf-8","Content-Disposition":`attachment; filename="${exported.filename}"`,"Cache-Control":"private, no-store","X-Content-Type-Options":"nosniff","X-Content-SHA256":exported.sha256}});
    }
    if(parts[0]==="tax-register"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await vatWorkspace(actor,{chartId:search.get("chartId")??undefined,filingMonth:search.get("filingMonth")??undefined})}));
    if(parts[0]==="tax-entries"&&parts.length===2)return Response.json(canonicalDestinationLinks({data:await taxEntryDetail(actor,parts[1])}));
    if(parts[0]==="vat-corrections"&&parts.length===2)return Response.json(canonicalDestinationLinks({data:await vatCorrectionDetail(actor,parts[1])}));
    if(parts[0]==="vat-packages"&&parts.length===2)return Response.json(canonicalDestinationLinks({data:await vatPackageDetail(actor,parts[1])}));
    if(parts[0]==="vat-packages"&&parts.length===3&&parts[2]==="export"){
      const exported=await vatPackageExport(actor,parts[1]);
      return new Response(exported.content,{headers:{"Content-Type":"text/csv; charset=utf-8","Content-Disposition":`attachment; filename="${exported.filename}"`,"Cache-Control":"private, no-store","X-Content-Type-Options":"nosniff"}});
    }
    if (parts[0] === "charts") {
      if (parts[1] && parts[2] === "accounts") return Response.json(canonicalDestinationLinks({ data: await listAccounts(actor, parts[1]) }));
      return Response.json(canonicalDestinationLinks({ data: await listCharts(actor) }));
    }
    if(parts[0]==="supplier-batches"&&parts.length===3&&parts[2]==="export")return Response.json(canonicalDestinationLinks({data:await recoverSupplierExport(actor,parts[1])}));
    if(parts[0]==="supplier-batches")return Response.json(canonicalDestinationLinks({data:parts[1]?await supplierBatchWorkspace(actor,parts[1]):await listSupplierBatches(actor)}));
    if(parts[0]==="refund-instructions"&&parts.length===3&&parts[2]==="export")return Response.json(canonicalDestinationLinks({data:await recoverRefundExport(actor,parts[1])}));
    if(parts[0]==="refunds")return Response.json(canonicalDestinationLinks({data:parts[1]?await refundWorkspace(actor,parts[1]):await listRefunds(actor)}));
    if (parts[0] === "periods") return Response.json(canonicalDestinationLinks({ data: await listPeriods(actor) }));
    if (parts[0] === "journals") return Response.json(canonicalDestinationLinks({ data: await listJournals(actor, { periodId: search.get("periodId") ?? undefined, status: search.get("status") ?? undefined }) }));
    if (parts[0] === "project-cost-allocations") {
      const projectId = search.get("projectId");
      if (!projectId) throw new ApiError(422, "PROJECT_ID_REQUIRED", "Select a project for project cost allocations.");
      return Response.json(canonicalDestinationLinks({ data: await listProjectCostAllocations(actor, projectId) }));
    }
    if (parts[0] === "project-cost-sources") {
      const projectId = search.get("projectId");
      if (!projectId) throw new ApiError(422, "PROJECT_ID_REQUIRED", "Select a project for posted cost sources.");
      return Response.json(canonicalDestinationLinks({ data: await listProjectCostSources(actor, projectId) }));
    }
    if (parts[0] === "invoices") return parts[1] ? Response.json(canonicalDestinationLinks({ data: await getInvoice(actor, parts[1]) })) : Response.json(canonicalDestinationLinks({ data: await listInvoices(actor) }));
    if (parts[0] === "payments") return parts[1] ? Response.json(canonicalDestinationLinks({ data: await getPayment(actor, parts[1]) })) : Response.json(canonicalDestinationLinks({ data: await listPayments(actor) }));
    if (parts[0] === "trial-balance") return Response.json(canonicalDestinationLinks({ data: await trialBalance(actor, search.get("periodId") ?? undefined,search.get("chartId")??undefined,reportDateSchema.parse({startDate:search.get("startDate")??undefined,endDate:search.get("endDate")??undefined})) }));
    if (parts[0] === "statements") return Response.json(canonicalDestinationLinks({ data: await statements(actor, search.get("periodId") ?? undefined,search.get("chartId")??undefined,reportDateSchema.parse({startDate:search.get("startDate")??undefined,endDate:search.get("endDate")??undefined})) }));
    if (parts[0] === "aging") { const kind = search.get("kind"); if (kind !== "payable" && kind !== "receivable") throw new ApiError(422, "INVALID_AGING_KIND", "kind must be payable or receivable."); return Response.json(canonicalDestinationLinks({ data: await aging(actor, kind, search.get("asOf") ?? new Date().toISOString().slice(0, 10),search.get("chartId")??undefined) })); }
    if(parts[0]==="engines")return Response.json(canonicalDestinationLinks({data:await financeEngineWorkspace(actor)}));
    if(parts[0]==="intercompany")return Response.json(canonicalDestinationLinks({data:await intercompanyWorkspace(actor)}));
    if(parts[0]==="group-entities")return Response.json(canonicalDestinationLinks({data:await intercompanyWorkspace(actor)}));
    if(parts[0]==="consolidation-runs")return Response.json(canonicalDestinationLinks({data:await consolidationRunWorkspace(actor)}));
    if(parts[0]==="accrual-schedules")return Response.json(canonicalDestinationLinks({data:await accrualSchedulePage(actor)}));
    if(parts[0]==="calculation-runs"&&parts[1])return Response.json(canonicalDestinationLinks({data:await calculationRunDetail(actor,parts[1])}));
    throw new ApiError(404, "NOT_FOUND", "Finance resource not found.");
  } catch (error) { return errorResponse(error); }
}

export async function POST(request: Request, context: FinanceRouteContext) {
  try {
    const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const { holdingSlug, path } = await context.params;
    const parts = segments(path);
    if (parts.length === 1 && parts[0] === "journals") {
      const response = await nativeJournalCreate(request, holdingSlug);
      if (response) return response;
    }
    if (parts.length === 3 && parts[0] === "journals" && parts[2] === "reverse") {
      const response = await nativeJournalReverse(request, holdingSlug, parts[1]);
      if (response) return response;
    }
    if (parts.length === 3 && parts[0] === "journals" && parts[2] === "post") {
      const response = await nativeJournalPost(request, holdingSlug, parts[1]);
      if (response) return response;
    }
    const actor = requirePermission(await requireWorkContext(holdingSlug),"finance.read");
    const input = await body(request);
    if(parts[0]?.startsWith('expense-setup')){
      if(parts[0]!=='expense-setup'||parts.length!==2)throw new ApiError(404,'NOT_FOUND','Expense setup resource not found.');
      const key=needCommand(request);
      const result=parts[1]==='charts'?await createExpensePostingChart(actor,expensePostingChartSchema.parse(input),key):parts[1]==='accounts'?await createExpensePostingAccount(actor,expensePostingAccountSchema.parse(input),key):parts[1]==='periods'?await createExpensePostingPeriod(actor,expensePostingPeriodSchema.parse(input),key):parts[1]==='rates'?await confirmExpensePostingRate(actor,expensePostingRateSchema.parse(input),key):parts[1]==='confirmations'?await expensePostingPrerequisiteConfirmation(actor,expensePostingPrerequisiteConfirmationSchema.parse(input),key):parts[1]==='mapping'?await initializeExpensePostingSetup(actor,expensePostingSetupSchema.parse(input),key):null;
      if(!result)throw new ApiError(404,'NOT_FOUND','Expense setup resource not found.');
      return Response.json(canonicalDestinationLinks({data:result}),{status:parts[1]==='confirmations'?200:201,headers:{'Cache-Control':'private, no-store'}});
    }
    if(parts[0]==="zero-usage-bills"&&parts.length===3&&["approve","reject"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideNoChargeBilling(actor,parts[1],businessDecisionSchema.parse(input),parts[2] as "approve"|"reject",needCommand(request))}));
    if(parts[0]==="due-drafts"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await scheduleFinanceDraft(actor,financeDueDraftSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="due-drafts"&&parts.length===3&&["retry","cancel"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideFinanceDueDraft(actor,parts[1],financeDueDecisionSchema.parse(input),parts[2] as "retry"|"cancel",needCommand(request))}));
    if(parts[0]==="instruments"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareInstrument(actor,instrumentEditionSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="instruments"&&parts.length===3&&["approve","reject"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideInstrument(actor,parts[1],instrumentDecisionSchema.parse(input),parts[2] as "approve"|"reject",needCommand(request))}));
    if(parts[0]==="instruments"&&parts.length===3&&parts[2]==="runs")return Response.json(canonicalDestinationLinks({data:await prepareInstrumentRun(actor,parts[1],instrumentRunSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="instrument-runs"&&parts.length===3&&["post","reject"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideInstrumentRun(actor,parts[1],instrumentDecisionSchema.parse(input),parts[2] as "post"|"reject",needCommand(request))}));
    if(parts[0]==="instrument-runs"&&parts.length===3&&parts[2]==="reverse")return Response.json(canonicalDestinationLinks({data:await reverseInstrumentRun(actor,parts[1],instrumentReversalSchema.parse(input),needCommand(request))}));
    if(parts[0]==="asset-policies"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareAssetPolicy(actor,assetPolicySchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="asset-policies"&&parts.length===3&&["approve","reject"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideAssetPolicy(actor,parts[1],assetPolicyDecisionSchema.parse(input),parts[2] as "approve"|"reject",needCommand(request))}));
    if(parts[0]==="asset-policies"&&parts.length===3&&parts[2]==="runs")return Response.json(canonicalDestinationLinks({data:await prepareAssetPolicyRun(actor,parts[1],assetPolicyRunSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="asset-policy-runs"&&parts.length===3&&["post","reject"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideAssetPolicyRun(actor,parts[1],assetPolicyDecisionSchema.parse(input),parts[2] as "post"|"reject",needCommand(request))}));
    if(parts[0]==="asset-policy-runs"&&parts.length===3&&parts[2]==="reverse")return Response.json(canonicalDestinationLinks({data:await reverseAssetPolicyRun(actor,parts[1],assetPolicyReversalSchema.parse(input),needCommand(request))}));
    if(parts[0]==="book-adjustments"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareBookAdjustment(actor,bookAdjustmentSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="book-adjustments"&&parts.length===3&&["post","reject"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideBookAdjustment(actor,parts[1],financeReviewSchema.parse(input),parts[2] as "post"|"reject",needCommand(request))}));
    if(parts[0]==="book-adjustments"&&parts.length===3&&parts[2]==="reverse")return Response.json(canonicalDestinationLinks({data:await reverseBookAdjustment(actor,parts[1],bookAdjustmentReversalSchema.parse(input),needCommand(request))}));
    if(parts[0]==="business-contracts"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareBusinessContract(actor,businessContractSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="business-contracts"&&parts.length===3&&["approve","reject","cancel"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideBusinessContract(actor,parts[1],businessDecisionSchema.parse(input),parts[2] as "approve"|"reject"|"cancel",needCommand(request))}));
    if(parts[0]==="business-contracts"&&parts.length===3&&parts[2]==="bill")return Response.json(canonicalDestinationLinks({data:await prepareBusinessBilling(actor,parts[1],businessBillingSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="business-bills"&&parts.length===3&&parts[2]==="corrections")return Response.json(canonicalDestinationLinks({data:await prepareBillingCorrection(actor,parts[1],billingCorrectionSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="billing-corrections"&&parts.length===3&&['post','reject'].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideBillingCorrection(actor,parts[1],billingCorrectionDecisionSchema.parse(input),parts[2] as 'post'|'reject',needCommand(request))}));
    if(parts[0]==="business-bills"&&parts.length===3&&parts[2]==="revisions")return Response.json(canonicalDestinationLinks({data:await prepareBusinessBillingRevision(actor,parts[1],billingRevisionSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="recognition-plans"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareRecognitionRun(actor,recognitionRunSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="recognition-plans"&&parts.length===3&&["post","reject"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideRecognitionRun(actor,parts[1],businessDecisionSchema.parse(input),parts[2] as "post"|"reject",needCommand(request))}));
    if(parts[0]==="recognition-plans"&&parts.length===3&&parts[2]==="reverse")return Response.json(canonicalDestinationLinks({data:await reverseRecognitionRun(actor,parts[1],recognitionReversalSchema.parse(input),needCommand(request))}));
    if(parts[0]==="company-tools"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await setFinanceCompanyTool(actor,financeToolSchema.parse(input),needCommand(request))}));
    if(parts[0]==="group-editions"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareGroupReportEdition(actor,groupReportEditionSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="group-editions"&&parts.length===3&&["approve","reject"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideGroupReportEdition(actor,parts[1],groupReportDecisionSchema.parse(input),parts[2] as "approve"|"reject",needCommand(request))}));
    if(parts[0]==="ownership-editions"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareOwnershipEdition(actor,ownershipEditionSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="ownership-editions"&&parts.length===3&&["approve","reject"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideOwnershipEdition(actor,parts[1],groupEvidenceDecisionSchema.parse(input),parts[2] as "approve"|"reject",needCommand(request))}));
    if(parts[0]==="acquisition-editions"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareAcquisitionEdition(actor,acquisitionEditionSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="acquisition-editions"&&parts.length===3&&["approve","reject"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideAcquisitionEdition(actor,parts[1],groupEvidenceDecisionSchema.parse(input),parts[2] as "approve"|"reject",needCommand(request))}));
    if(parts[0]==="intercompany-pairs"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareIntercompanyPair(actor,intercompanyPairSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="intercompany-pairs"&&parts.length===3&&["confirm","reverse","reject"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideIntercompanyPair(actor,parts[1],intercompanyPairDecisionSchema.parse(input),parts[2] as "confirm"|"reverse"|"reject",needCommand(request))}));
    if(parts[0]==="bank-csv-mappings"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await createBankCsvMapping(actor,bankCsvMappingSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="bank-csv-imports"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await importMappedBankCsv(actor,bankCsvImportSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="bank-match-groups"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await proposeBankMatchGroup(actor,bankMatchGroupSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="bank-match-groups"&&parts.length===3&&["confirm","reverse","reject"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideBankMatchGroup(actor,parts[1],financeReviewSchema.parse(input),parts[2] as "confirm"|"reverse"|"reject",needCommand(request))}));
    if(parts[0]==="collection-cases"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareCollectionCase(actor,collectionCaseSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="collection-cases"&&parts.length===3&&parts[2]==="revise")return Response.json(canonicalDestinationLinks({data:await reviseCollectionCase(actor,parts[1],reviseCollectionCaseSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="collection-cases"&&parts.length===3&&["approve","cancel","test-deliver"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideCollectionCase(actor,parts[1],financeReviewSchema.parse(input),parts[2] as "approve"|"cancel"|"test-deliver",needCommand(request))}));
    if(parts[0]==="budget-revisions"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareBudgetEdition(actor,budgetEditionSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="budget-revisions"&&parts.length===2&&parts[1]==="import")return Response.json(canonicalDestinationLinks({data:await importBudgetCsv(actor,z.object({chartId:z.string().min(1),name:z.string().min(1),year:z.number().int(),csv:z.string().max(5_000_000),parentId:z.string().optional(),expectedVersion:z.number().int().positive().optional()}).parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="forecasts"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareForecastEdition(actor,forecastEditionSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="report-definitions"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareFinancialReport(actor,financialReportSchema.parse(input),needCommand(request))}),{status:201});
    if(["budget-revisions","forecasts","report-definitions"].includes(parts[0])&&parts.length===3&&(parts[2]==="approve"||parts[2]==="reject"))return Response.json(canonicalDestinationLinks({data:await decideFinancePlan(actor,parts[1],financeReviewSchema.parse(input),parts[2],needCommand(request))}));
    if(parts[0]==="report-definitions"&&parts.length===3&&parts[2]==="snapshot")return Response.json(canonicalDestinationLinks({data:await snapshotFinancialReport(actor,parts[1],z.object({startDate:z.string().date(),endDate:z.string().date()}).parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="dimensions"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await createFinanceDimension(actor,dimensionSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="book-mappings"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareBookRule(actor,bookRuleSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="book-mappings"&&parts.length===3&&(parts[2]==="approve"||parts[2]==="reject"))return Response.json(canonicalDestinationLinks({data:await decideBookRule(actor,parts[1],financeReviewSchema.parse(input),parts[2],needCommand(request))}));
    if(parts[0]==="tax-identities"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareTaxIdentity(actor,taxIdentitySchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="tax-identities"&&parts.length===2)return Response.json(canonicalDestinationLinks({data:await prepareTaxIdentity(actor,taxIdentitySchema.parse(input),needCommand(request),parts[1])}));
    if(parts[0]==="tax-identities"&&parts.length===3&&parts[2]==="decide")return Response.json(canonicalDestinationLinks({data:await decideTaxIdentity(actor,parts[1],taxIdentityDecisionSchema.parse(input),needCommand(request))}));
    if(parts[0]==="withholding-months"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareWithholdingPackage(actor,withholdingPackageSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="withholding-months"&&parts.length===3&&parts[2]==="decide")return Response.json(canonicalDestinationLinks({data:await decideWithholdingPackage(actor,parts[1],withholdingPackageDecisionSchema.parse(input),needCommand(request))}));
    if(parts[0]==="withholding-months"&&parts.length===3&&parts[2]==="remit")return Response.json(canonicalDestinationLinks({data:await remitWithholdingPackage(actor,parts[1],withholdingRemittanceSchema.parse(input),needCommand(request))}));
    if(parts[0]==="withholding-remittances"&&parts.length===3&&parts[2]==="reverse")return Response.json(canonicalDestinationLinks({data:await reverseWithholdingRemittance(actor,parts[1],withholdingRemittanceReversalSchema.parse(input),needCommand(request))}));
    if(parts[0]==="payments"&&parts.length===3&&parts[2]==="withholding")return Response.json(canonicalDestinationLinks({data:await prepareWithholdingAssessment(actor,parts[1],withholdingAssessmentSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="withholding"&&parts.length===3&&parts[2]==="decide")return Response.json(canonicalDestinationLinks({data:await decideWithholdingAssessment(actor,parts[1],withholdingDecisionSchema.parse(input),needCommand(request))}));
    if(parts[0]==="invoices"&&parts.length===3&&parts[2]==="issue-thai-tax-invoice")return Response.json(canonicalDestinationLinks({data:await issueThaiTaxInvoice(actor,parts[1],thaiInvoiceIssueSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="thai-documents"&&parts.length===3&&parts[2]==="changes")return Response.json(canonicalDestinationLinks({data:await prepareThaiDocumentChange(actor,parts[1],thaiDocumentChangeSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="thai-document-changes"&&parts.length===3&&parts[2]==="decide")return Response.json(canonicalDestinationLinks({data:await decideThaiDocumentChange(actor,parts[1],thaiDocumentDecisionSchema.parse(input),needCommand(request))}));
    if(parts[0]==="statement-packages"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareStatementPackage(actor,statementPackageSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="statement-packages"&&parts.length===3&&["issue","reject","cancel"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideStatementPackage(actor,parts[1],parts[2] as "issue"|"reject"|"cancel",reportingDecisionSchema.parse(input),needCommand(request))}));
    if(parts[0]==="year-closings"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareYearClosing(actor,yearClosingSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="year-closings"&&parts.length===3&&["post","reject","cancel"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideYearClosing(actor,parts[1],parts[2] as "post"|"reject"|"cancel",reportingDecisionSchema.parse(input),needCommand(request))}));
    if(parts[0]==="ai") {
      const {write}=await holdingResources(actor.holdingId),env=await runtimeEnv(),key=needCommand(request);
      if(parts.length===2&&parts[1]==="policy")return Response.json(canonicalDestinationLinks({data:await saveFinanceAiPolicy(write,actor,env,input,key)}));
      if(parts.length===2&&parts[1]==="prepare")return Response.json(canonicalDestinationLinks({data:await queueFinanceAi(write,actor,env,input,key)}),{status:202});
      if(parts.length===3&&parts[2]==="dismiss")return Response.json(canonicalDestinationLinks({data:await dismissFinanceAi(write,actor,parts[1],financeAiDecisionSchema.parse(input),key)}));
      if(parts.length===3&&parts[2]==="apply-draft")return Response.json(canonicalDestinationLinks({data:await applyFinanceAiDraft(write,actor,parts[1],input,key)}));
      throw new ApiError(404,"NOT_FOUND","Finance AI action not found.");
    }
    if(parts[0]==="accounting-profiles"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareAccountingProfile(actor,accountingProfileSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="accounting-profiles"&&parts.length===3&&(parts[2]==="approve"||parts[2]==="reject"))return Response.json(canonicalDestinationLinks({data:await decideAccountingProfile(actor,parts[1],parts[2],accountingProfileDecisionSchema.parse(input),needCommand(request))}));
    if(parts[0]==="tax-entries"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareVatEntry(actor,vatEntrySchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="tax-entries"&&parts.length===3&&parts[2]==="corrections")return Response.json(canonicalDestinationLinks({data:await prepareVatCorrection(actor,parts[1],vatCorrectionSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="vat-corrections"&&parts.length===3&&(parts[2]==="approve"||parts[2]==="reject"))return Response.json(canonicalDestinationLinks({data:await reviewVatCorrection(actor,parts[1],parts[2],taxReviewSchema.parse(input),needCommand(request))}));
    if(parts[0]==="tax-entries"&&parts.length===3&&(parts[2]==="approve"||parts[2]==="reject"))return Response.json(canonicalDestinationLinks({data:await reviewVatEntry(actor,parts[1],parts[2],taxReviewSchema.parse(input),needCommand(request))}));
    if(parts[0]==="vat-packages"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareVatPackage(actor,vatPackageSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="vat-packages"&&parts.length===3){
      if(parts[2]==="approve")return Response.json(canonicalDestinationLinks({data:await approveVatPackage(actor,parts[1],taxReviewSchema.parse(input),needCommand(request))}));
      if(parts[2]==="export")return Response.json(canonicalDestinationLinks({data:await exportVatWorkpaper(actor,parts[1],taxReviewSchema.parse(input),needCommand(request))}));
      if(parts[2]==="file")return Response.json(canonicalDestinationLinks({data:await recordVatFiling(actor,parts[1],vatFilingSchema.parse(input),needCommand(request))}));
      if(parts[2]==="supersede")return Response.json(canonicalDestinationLinks({data:await supersedeVatPackage(actor,parts[1],taxReviewSchema.parse(input),needCommand(request))}));
    }
    if(parts[0]==="project-cost-allocations"&&parts.length===1){const parsed=projectCostAllocationSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Invalid project cost allocation.");return Response.json(canonicalDestinationLinks({data:await createProjectCostAllocation(actor,parsed.data,needCommand(request))}),{status:201});}
    if(parts[0]==="project-cost-allocations"&&parts.length===3&&parts[2]==="review"){const parsed=projectCostAllocationDecisionSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Project cost review evidence is required.");return Response.json(canonicalDestinationLinks({data:await decideProjectCostAllocation(actor,parts[1],parsed.data,needCommand(request))}));}
    if(parts[0]==="calculation-runs"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await createCalculationRun(actor,calculationRunSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="calculation-runs"&&parts.length===3&&["review","approve","reject"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideCalculationRun(actor,parts[1],calculationDecisionSchema.parse(input),parts[2] as "review"|"approve"|"reject",needCommand(request))}));
    if(parts[0]==="intercompany"&&parts.length===1){const parsed=intercompanyTransactionSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Invalid intercompany transaction.");return Response.json(canonicalDestinationLinks({data:await createIntercompany(actor,parsed.data,needCommand(request))}),{status:201});}
    if(parts[0]==="intercompany"&&parts.length===3&&["match","settle","eliminate"].includes(parts[2])){const parsed=intercompanyDecisionSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Intercompany decision evidence is required.");return Response.json(canonicalDestinationLinks({data:await decideIntercompany(actor,parts[1],parts[2] as "match"|"settle"|"eliminate",parsed.data,needCommand(request))}));}
    if(parts[0]==="group-entities"&&parts.length===1){const parsed=groupEntitySchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Group entity details are invalid.");return Response.json(canonicalDestinationLinks({data:await createGroupEntity(actor,parsed.data,needCommand(request))}),{status:201});}
    if(parts[0]==="consolidation-runs"&&parts.length===3&&parts[2]==="eliminations"){const parsed=consolidationEliminationSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Consolidation elimination details are invalid.");return Response.json(canonicalDestinationLinks({data:await createConsolidationElimination(actor,parts[1],parsed.data,needCommand(request))}),{status:201});}
    if(parts[0]==="consolidation-eliminations"&&parts.length===3&&["approve","post"].includes(parts[2])){const parsed=consolidationEliminationDecisionSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Consolidation elimination decision evidence is required.");return Response.json(canonicalDestinationLinks({data:await decideConsolidationElimination(actor,parts[1],parts[2] as "approve"|"post",parsed.data,needCommand(request))}));}
    if(parts[0]==="consolidation-runs"&&parts.length===1){const parsed=consolidationRunSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Consolidation run details are invalid.");return Response.json(canonicalDestinationLinks({data:await createConsolidationRun(actor,parsed.data,needCommand(request))}),{status:201});}
    if(parts[0]==="consolidation-runs"&&parts.length===3&&["review","approve","publish"].includes(parts[2])){const parsed=consolidationDecisionSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Consolidation decision evidence is required.");return Response.json(canonicalDestinationLinks({data:await decideConsolidationRun(actor,parts[1],parts[2] as "review"|"approve"|"publish",parsed.data,needCommand(request))}));}
    if(parts[0]==="accrual-schedules"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await createAccrualSchedule(actor,accrualScheduleSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="accrual-schedules"&&parts.length===3&&["approve","cancel"].includes(parts[2]))return Response.json(canonicalDestinationLinks({data:await decideAccrualSchedule(actor,parts[1],accrualScheduleDecisionSchema.parse(input),parts[2] as "approve"|"cancel",needCommand(request))}));
    if(parts[0]==="accrual-schedules"&&parts.length===3&&parts[2]==="prepare")return Response.json(canonicalDestinationLinks({data:await prepareAccrualRun(actor,parts[1],needCommand(request))}),{status:201});
    if(parts[0]==="accrual-runs"&&parts.length===3&&parts[2]==="post")return Response.json(canonicalDestinationLinks({data:await postAccrualRun(actor,parts[1],accrualRunPostSchema.parse(input),needCommand(request))}));
    if(parts[0]==="fx-revaluations"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await prepareFxRevaluation(actor,prepareFxRevaluationSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="fx-revaluations"&&parts.length===3&&(parts[2]==="approve"||parts[2]==="reject"))return Response.json(canonicalDestinationLinks({data:await decideFxRevaluation(actor,parts[1],decideFxRevaluationSchema.parse(input),parts[2],needCommand(request))}));
    if(parts[0]==="periods"&&parts.length===3&&parts[2]==="close-checks")return Response.json(canonicalDestinationLinks({data:await snapshotCloseChecks(actor,parts[1],needCommand(request))}));
    if(parts[0]==="supplier-batches"&&parts.length===1)return Response.json(canonicalDestinationLinks({data:await createSupplierBatch(actor,supplierBatchSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="supplier-batches"&&parts.length===3&&(parts[2]==="approve"||parts[2]==="cancel"))return Response.json(canonicalDestinationLinks({data:await decideSupplierBatch(actor,parts[1],supplierDecisionSchema.parse(input),needCommand(request),parts[2])}));
    if(parts[0]==="supplier-batches"&&parts.length===3&&parts[2]==="export")return Response.json(canonicalDestinationLinks({data:await exportSupplierBatch(actor,parts[1],supplierExportSchema.parse(input),needCommand(request))}));
    if(parts[0]==="supplier-instructions"&&parts.length===3&&parts[2]==="result")return Response.json(canonicalDestinationLinks({data:await recordSupplierResult(actor,parts[1],supplierResultSchema.parse(input),needCommand(request))}));
    if(parts[0]==="supplier-instructions"&&parts.length===3&&parts[2]==="confirm")return Response.json(canonicalDestinationLinks({data:await confirmSupplierInstruction(actor,parts[1],supplierConfirmSchema.parse(input),needCommand(request))}));
    if(parts[0]==="refunds"&&parts.length===3&&parts[2]==="approve")return Response.json(canonicalDestinationLinks({data:await approveRefund(actor,parts[1],refundApprovalSchema.parse(input),needCommand(request))}));
    if(parts[0]==="refunds"&&parts.length===3&&parts[2]==="instructions")return Response.json(canonicalDestinationLinks({data:await prepareRefundInstruction(actor,parts[1],refundInstructionSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="refund-instructions"&&parts.length===2&&parts[1]==="export")return Response.json(canonicalDestinationLinks({data:await exportRefundInstructions(actor,refundExportSchema.parse(input),needCommand(request))}));
    if(parts[0]==="refund-instructions"&&parts.length===3&&parts[2]==="bank-evidence")return Response.json(canonicalDestinationLinks({data:await importRefundBankEvidence(actor,parts[1],refundBankEvidenceSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="refund-instructions"&&parts.length===3&&parts[2]==="confirm")return Response.json(canonicalDestinationLinks({data:await confirmRefundInstruction(actor,parts[1],refundConfirmationSchema.parse(input),needCommand(request))}));
    if(parts[0]==="refund-instructions"&&parts.length===3&&parts[2]==="exception")return Response.json(canonicalDestinationLinks({data:await recordRefundException(actor,parts[1],refundExceptionSchema.parse(input),needCommand(request))}));
    if (parts[0] === "charts" && parts[1] === "import") {
      const normalized = typeof input === "object" && input !== null && "csv" in input && typeof input.csv === "string" ? { ...(input as Record<string, unknown>), rows: csvRows(input.csv) } : input;
      const parsed = z.object({ chartId: z.string().min(1), rows: z.array(z.object({ code: z.string(), name: z.string(), category: z.enum(["asset", "liability", "equity", "revenue", "expense", "other_income", "other_expense"]), currency: z.string().optional(), controlKind:z.enum(["none","receivable","payable","bank"]).optional() })).min(1) }).safeParse(normalized);
      if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid chart import.");
      return Response.json(canonicalDestinationLinks({ data: await importAccounts(actor, parsed.data.chartId, parsed.data.rows, needCommand(request)) }), { status: 201 });
    }
    if(parts[0]==="setup"&&parts[1]==="starter"&&parts.length===2)return Response.json(canonicalDestinationLinks({data:await createFinanceStarter(actor,financeStarterSchema.parse(input),needCommand(request))}),{status:201});
    if (parts[0] === "charts" && parts.length === 1) { const parsed = createChartSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid chart."); return Response.json(canonicalDestinationLinks({ data: await createChart(actor, parsed.data, needCommand(request)) }), { status: 201 }); }
    if (parts[0] === "charts" && parts[2] === "accounts" && parts[4] === "status" && parts.length === 5) { const parsed = changeAccountStatusSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid account status change."); return Response.json(canonicalDestinationLinks({ data: await changeAccountStatus(actor, parts[1], parts[3], parsed.data, needCommand(request)) })); }
    if (parts[0] === "charts" && parts[2] === "accounts" && parts.length === 3) { const parsed = createAccountSchema.safeParse({ ...(typeof input === "object" && input !== null ? input : {}), chartId: parts[1] }); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid account."); return Response.json(canonicalDestinationLinks({ data: await createAccount(actor, parsed.data, needCommand(request)) }), { status: 201 }); }
    if (parts[0] === "periods" && parts.length === 1) { const parsed = createPeriodSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid period."); return Response.json(canonicalDestinationLinks({ data: await createPeriod(actor, parsed.data, needCommand(request)) }), { status: 201 }); }
    if (parts[0] === "journals" && parts.length === 1) { const parsed = createJournalSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid journal."); return Response.json(canonicalDestinationLinks({ data: await createJournal(actor, {...parsed.data,sourceType:"manual",sourceId:undefined,isOpening:false}, needCommand(request)) }), { status: 201 }); }
    if (parts[0] === "opening-balances" && parts[1] === "preview") { const parsed = openingTrialBalanceSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Opening trial balance is invalid."); return Response.json(canonicalDestinationLinks({ data: await previewOpeningTrialBalance(actor, parsed.data) })); }
    if (parts[0] === "opening-balances" && parts[1] === "import") { const parsed = openingTrialBalanceSchema.extend({previewToken:z.string().length(64)}).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Opening trial balance is invalid."); return Response.json(canonicalDestinationLinks({ data: await importOpeningTrialBalance(actor, parsed.data, needCommand(request)) }), { status: 201 }); }
    if(parts[0]==="journals"&&parts.length===3&&parts[2]==="edit")return Response.json(canonicalDestinationLinks({data:await editManualDraft(actor,parts[1],manualDraftEditSchema.parse(input),needCommand(request))}));
    if(parts[0]==="journals"&&parts.length===2&&parts[1]==="batch-post")return Response.json(canonicalDestinationLinks({data:await postJournalBatch(actor,journalBatchSchema.parse(input),needCommand(request))}));
    if (parts[0] === "journals" && parts[2] === "post") { const parsed = z.object({ expectedVersion: z.number().int().positive() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "expectedVersion is required."); return Response.json(canonicalDestinationLinks({ data: await postJournal(actor, parts[1], needCommand(request), parsed.data) })); }
    if (parts[0] === "journals" && parts[2] === "reverse") { const parsed = z.object({ periodId: z.string(), entryDate: z.string().date(), memo: z.string().optional() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "A reversal period and date are required."); return Response.json(canonicalDestinationLinks({ data: await reverseJournal(actor, parts[1], needCommand(request), parsed.data) }), { status: 201 }); }
    if(parts[0]==="periods"&&parts.length===3&&parts[2]==="cancel-close")return Response.json(canonicalDestinationLinks({data:await cancelClose(actor,parts[1],cancelCloseSchema.parse(input),needCommand(request))}));
    if (parts[0] === "periods" && parts[2] === "begin-close") return Response.json(canonicalDestinationLinks({ data: await beginClose(actor, parts[1], needCommand(request)) }));
    if (parts[0] === "periods" && parts[2] === "close") { const parsed = closeProofSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Close evidence is required."); return Response.json(canonicalDestinationLinks({ data: await closePeriod(actor, parts[1], parsed.data, needCommand(request)) })); }
    if (parts[0] === "periods" && parts[2] === "reopen") { const parsed = closeProofSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Reopen evidence is required."); return Response.json(canonicalDestinationLinks({ data: await reopenPeriod(actor, parts[1], parsed.data, needCommand(request)) })); }
    if (parts[0] === "parties" && parts.length === 1) { const parsed = partySchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid party."); return Response.json(canonicalDestinationLinks({ data: await createParty(actor, parsed.data, needCommand(request)) }), { status: 201 }); }
    if (parts[0] === "invoices" && parts.length === 1) { const parsed = invoiceSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid invoice."); return Response.json(canonicalDestinationLinks({ data: await createInvoice(actor, parsed.data, needCommand(request)) }), { status: 201 }); }
    if(parts[0]==="invoices"&&parts.length===2){const parsed=invoiceSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0].message);return Response.json(canonicalDestinationLinks({data:await createInvoice(actor,parsed.data,needCommand(request),parts[1])}));}
    if(parts[0]==="invoices"&&parts.length===3&&parts[2]==="confirm-no-charge")return Response.json(canonicalDestinationLinks({data:await confirmServiceNoCharge(actor,parts[1],input,needCommand(request))}));
    if(parts[0]==="invoices"&&parts[2]==="void"){const data=z.object({expectedVersion:z.number().int().positive()}).parse(input);return Response.json(canonicalDestinationLinks({data:await voidDraftInvoice(actor,parts[1],data.expectedVersion,needCommand(request))}));}
    if (parts[0] === "invoices" && parts[2] === "post") { const parsed = invoicePostingSchema.extend({expectedVersion:z.number().int().positive()}).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Posting period, date and a valid exchange rate are required."); return Response.json(canonicalDestinationLinks({ data: await postInvoice(actor, parts[1], parsed.data, needCommand(request)) })); }
    if (parts[0] === "payments" && parts.length === 1) { const parsed = z.object({ partyId: z.string().optional(), kind: z.enum(["payment", "receipt", "refund"]), paymentNumber: z.string(), paymentDate: z.string().date(), currency: z.string().length(3), amount: z.string(), bankAccount: z.string().optional(), settlementInvoiceId:z.string().min(1).max(128).optional(),correctionPaymentId:z.string().min(1).max(128).optional() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid payment."); return Response.json(canonicalDestinationLinks({ data: await createPayment(actor, parsed.data, needCommand(request)) }), { status: 201 }); }
    if (parts[0] === "payments" && parts[2] === "approve") { const parsed = z.object({ expectedVersion: z.number().int().positive(), proof: z.string() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Payment approval evidence is required."); return Response.json(canonicalDestinationLinks({ data: await approvePayment(actor, parts[1], parsed.data, needCommand(request)) })); }
    if (parts[0] === "payments" && parts[2] === "release") { const parsed = z.object({ expectedVersion: z.number().int().positive(), proof: z.string() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Payment release evidence is required."); return Response.json(canonicalDestinationLinks({ data: await releasePayment(actor, parts[1], parsed.data, needCommand(request)) })); }
    if (parts[0] === "payments" && parts[2] === "confirm") { const parsed = confirmPaymentSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message??"Payment confirmation is invalid."); return Response.json(canonicalDestinationLinks({ data: await confirmPayment(actor, parts[1], parsed.data, needCommand(request)) })); }
    if (parts[0] === "payments" && parts[2] === "reverse") { const parsed = reversePaymentSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message??"Payment correction evidence is required."); return Response.json(canonicalDestinationLinks({ data: await reversePayment(actor, parts[1], parsed.data, needCommand(request)) })); }
    if (parts[0] === "payments" && parts[2] === "allocate") { const parsed = z.object({ invoiceId: z.string(), amount: z.string(), expectedVersion: z.number().int().positive() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Payment allocation is invalid."); return Response.json(canonicalDestinationLinks({ data: await allocatePayment(actor, parts[1], parsed.data, needCommand(request)) })); }
    if (parts[0] === "payments" && parts[1] === "export") { const parsed = z.object({ paymentIds: z.array(z.string()).min(1) }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Select released payments."); return Response.json(canonicalDestinationLinks({ data: await exportPaymentFile(actor, parsed.data.paymentIds, needCommand(request)) })); }
    if (parts[0] === "tax-codes" && parts.length === 1) { const parsed = taxCodeSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid tax code."); return Response.json(canonicalDestinationLinks({ data: await createTaxCode(actor, parsed.data, needCommand(request)) }), { status: 201 }); }
    if (parts[0] === "assets" && parts.length === 1) { const parsed = assetSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid asset."); return Response.json(canonicalDestinationLinks({ data: await createAsset(actor, parsed.data, needCommand(request)) }), { status: 201 }); }
    if(parts[0]==="assets"&&parts.length===3&&parts[2]==="edit")return Response.json(canonicalDestinationLinks({data:await editAsset(actor,parts[1],assetEditSchema.parse(input),needCommand(request))}));
    if(parts[0]==="assets"&&parts.length===3&&parts[2]==="cancel")return Response.json(canonicalDestinationLinks({data:await cancelDraftAsset(actor,parts[1],assetDecisionSchema.parse(input),needCommand(request))}));
    if(parts[0]==="assets"&&parts.length===3&&parts[2]==="activate")return Response.json(canonicalDestinationLinks({data:await activateAsset(actor,parts[1],assetActivationSchema.parse(input),needCommand(request))}));
    if(parts[0]==="assets"&&parts.length===3&&parts[2]==="depreciate")return Response.json(canonicalDestinationLinks({data:await depreciateAsset(actor,parts[1],assetDepreciationSchema.parse(input),needCommand(request))}));
    if(parts[0]==="assets"&&parts.length===3&&parts[2]==="disposals")return Response.json(canonicalDestinationLinks({data:await prepareAssetDisposal(actor,parts[1],assetDisposalSchema.parse(input),needCommand(request))}),{status:201});
    if(parts[0]==="asset-disposals"&&parts.length===3&&(parts[2]==="post"||parts[2]==="cancel"))return Response.json(canonicalDestinationLinks({data:await decideAssetDisposal(actor,parts[1],parts[2],assetDecisionSchema.parse(input),needCommand(request))}));
    if (parts[0] === "budgets" && parts.length === 1) { const parsed = budgetSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid budget."); return Response.json(canonicalDestinationLinks({ data: await createBudget(actor, parsed.data, needCommand(request)) }), { status: 201 }); }
    if (parts[0] === "expenses") throw new ApiError(410,"EXPENSE_API_MOVED","Use the /expenses workflow for employee obligations.");
    if (parts[0] === "leases" && parts.length === 1) { const parsed = leaseSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid lease."); return Response.json(canonicalDestinationLinks({ data: await createLease(actor, parsed.data, needCommand(request)) }), { status: 201 }); }
    if (parts[0] === "loans" && parts.length === 1) { const parsed = loanSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid loan."); return Response.json(canonicalDestinationLinks({ data: await createLoan(actor, parsed.data, needCommand(request)) }), { status: 201 }); }
    if (parts[0] === "revenue-contracts" && parts.length === 1) { const parsed = revenueContractSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid revenue contract."); return Response.json(canonicalDestinationLinks({ data: await createRevenueContract(actor, parsed.data, needCommand(request)) }), { status: 201 }); }
    if (parts[0] === "revenue-obligations" && parts.length === 1) { const parsed = revenueObligationSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid revenue obligation."); return Response.json(canonicalDestinationLinks({ data: await createRevenueObligation(actor, parsed.data, needCommand(request)) }), { status: 201 }); }
    if (parts[0] === "revenue-obligations" && parts[2] === "recognize") { const parsed = z.object({ amount: z.string(), periodId: z.string(), entryDate: z.string().date() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Revenue recognition period, date and amount are required."); return Response.json(canonicalDestinationLinks({ data: await recognizeRevenue(actor, parts[1], parsed.data, needCommand(request)) })); }
    if (parts[0] === "bank" && parts[1] === "statements" && parts[2] === "import") { const parsed = z.object({ settlementPaymentId:z.string().min(1).optional(), bankAccount: z.string(), statementNumber: z.string(), statementDate: z.string().date(), opening: z.string(), closing: z.string(), closingFxRate:z.string().optional(), sourceHash: z.string().min(8), rows: z.array(z.object({ externalId: z.string(), transactionDate: z.string().date(), description: z.string().optional(), amount: z.string(), currency: z.string().length(3) })).max(500) }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid bank statement."); return Response.json(canonicalDestinationLinks({ data: await importBankStatement(actor, parsed.data, needCommand(request)) }), { status: 201 }); }
    if (parts[0] === "bank" && parts[1] === "transactions" && parts[3] === "reconcile") { const parsed = z.object({ expectedVersion: z.number().int().positive(), matchedType: z.string(), matchedId: z.string(), proof: z.string(), attachmentSetIds: z.array(z.string().min(1)).max(20).default([]) }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Reconciliation evidence is required."); return Response.json(canonicalDestinationLinks({ data: await reconcileBankTransaction(actor, parts[2], parsed.data, needCommand(request)) })); }
    throw new ApiError(404, "NOT_FOUND", "Finance resource not found.");
  } catch (error) { return errorResponse(error); }
}

async function listPeriods(actor: import("@erpx/server-identity_access/public/authorization").WorkContext) {
  const { holdingResources } = await import("../../../../../../../../../../packages/server-runtime/src/public/runtime");
  const { write } = await holdingResources(actor.holdingId);
  return financeEntrypointQueries(write).allPeriods(actor.companyId);
}
