import { requestPublicOrigin } from "@/server/public-origin";
import { requireWorkContext } from "@/server/authorization";
import { ApiError, errorResponse } from "@/server/http";
import {
  aging,
  beginClose,
  closePeriod,
  createAccount,
  createAccountSchema,
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
} from "@/server/finance";
import { z } from "zod";

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
    const actor = await requireWorkContext(holdingSlug);
    const parts = segments(path); const search = new URL(request.url).searchParams;
    if (parts[0] === "charts") {
      if (parts[1] && parts[2] === "accounts") return Response.json({ data: await listAccounts(actor, parts[1]) });
      return Response.json({ data: await listCharts(actor) });
    }
    if (parts[0] === "periods") return Response.json({ data: await listPeriods(actor) });
    if (parts[0] === "journals") return Response.json({ data: await listJournals(actor, { periodId: search.get("periodId") ?? undefined, status: search.get("status") ?? undefined }) });
    if (parts[0] === "invoices") return parts[1] ? Response.json({ data: await getInvoice(actor, parts[1]) }) : Response.json({ data: await listInvoices(actor) });
    if (parts[0] === "payments") return parts[1] ? Response.json({ data: await getPayment(actor, parts[1]) }) : Response.json({ data: await listPayments(actor) });
    if (parts[0] === "trial-balance") return Response.json({ data: await trialBalance(actor, search.get("periodId") ?? undefined,search.get("chartId")??undefined) });
    if (parts[0] === "statements") return Response.json({ data: await statements(actor, search.get("periodId") ?? undefined,search.get("chartId")??undefined) });
    if (parts[0] === "aging") { const kind = search.get("kind"); if (kind !== "payable" && kind !== "receivable") throw new ApiError(422, "INVALID_AGING_KIND", "kind must be payable or receivable."); return Response.json({ data: await aging(actor, kind, search.get("asOf") ?? new Date().toISOString().slice(0, 10)) }); }
    throw new ApiError(404, "NOT_FOUND", "Finance resource not found.");
  } catch (error) { return errorResponse(error); }
}

export async function POST(request: Request, context: FinanceRouteContext) {
  try {
    const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const { holdingSlug, path } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const parts = segments(path); const input = await body(request);
    if (parts[0] === "charts" && parts[1] === "import") {
      const normalized = typeof input === "object" && input !== null && "csv" in input && typeof input.csv === "string" ? { ...(input as Record<string, unknown>), rows: csvRows(input.csv) } : input;
      const parsed = z.object({ chartId: z.string().min(1), rows: z.array(z.object({ code: z.string(), name: z.string(), category: z.enum(["asset", "liability", "equity", "revenue", "expense", "other_income", "other_expense"]), currency: z.string().optional() })).min(1) }).safeParse(normalized);
      if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid chart import.");
      return Response.json({ data: await importAccounts(actor, parsed.data.chartId, parsed.data.rows, needCommand(request)) }, { status: 201 });
    }
    if (parts[0] === "charts" && parts.length === 1) { const parsed = createChartSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid chart."); return Response.json({ data: await createChart(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "charts" && parts[2] === "accounts" && parts.length === 3) { const parsed = createAccountSchema.safeParse({ ...(typeof input === "object" && input !== null ? input : {}), chartId: parts[1] }); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid account."); return Response.json({ data: await createAccount(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "periods" && parts.length === 1) { const parsed = createPeriodSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid period."); return Response.json({ data: await createPeriod(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "journals" && parts.length === 1) { const parsed = createJournalSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid journal."); return Response.json({ data: await createJournal(actor, {...parsed.data,sourceType:"manual",sourceId:undefined}, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "opening-balances" && parts[1] === "import") { const parsed = z.object({ chartId: z.string(), periodId: z.string(), entryDate: z.string().date(), currency: z.string().length(3), lines: z.array(z.object({ accountId: z.string(), debit: z.string(), credit: z.string(), description: z.string().optional() })).min(2) }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Opening trial balance is invalid."); return Response.json({ data: await importOpeningTrialBalance(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "journals" && parts[2] === "post") { const parsed = z.object({ expectedVersion: z.number().int().positive() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "expectedVersion is required."); return Response.json({ data: await postJournal(actor, parts[1], needCommand(request), parsed.data) }); }
    if (parts[0] === "journals" && parts[2] === "reverse") { const parsed = z.object({ periodId: z.string(), entryDate: z.string().date(), memo: z.string().optional() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "A reversal period and date are required."); return Response.json({ data: await reverseJournal(actor, parts[1], needCommand(request), parsed.data) }, { status: 201 }); }
    if (parts[0] === "periods" && parts[2] === "begin-close") return Response.json({ data: await beginClose(actor, parts[1], needCommand(request)) });
    if (parts[0] === "periods" && parts[2] === "close") { const parsed = closeProofSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Close evidence is required."); return Response.json({ data: await closePeriod(actor, parts[1], parsed.data, needCommand(request)) }); }
    if (parts[0] === "periods" && parts[2] === "reopen") { const parsed = closeProofSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Reopen evidence is required."); return Response.json({ data: await reopenPeriod(actor, parts[1], parsed.data, needCommand(request)) }); }
    if (parts[0] === "parties" && parts.length === 1) { const parsed = partySchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid party."); return Response.json({ data: await createParty(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "invoices" && parts.length === 1) { const parsed = invoiceSchema.safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid invoice."); return Response.json({ data: await createInvoice(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if(parts[0]==="invoices"&&parts.length===2){const parsed=invoiceSchema.safeParse(input);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0].message);return Response.json({data:await createInvoice(actor,parsed.data,needCommand(request),parts[1])});}
    if(parts[0]==="invoices"&&parts[2]==="void"){const data=z.object({expectedVersion:z.number().int().positive()}).parse(input);return Response.json({data:await voidDraftInvoice(actor,parts[1],data.expectedVersion,needCommand(request))});}
    if (parts[0] === "invoices" && parts[2] === "post") { const parsed = z.object({ periodId: z.string(), entryDate: z.string().date() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Posting period and date are required."); return Response.json({ data: await postInvoice(actor, parts[1], parsed.data, needCommand(request)) }); }
    if (parts[0] === "payments" && parts.length === 1) { const parsed = z.object({ partyId: z.string().optional(), kind: z.enum(["payment", "receipt", "refund"]), paymentNumber: z.string(), paymentDate: z.string().date(), currency: z.string().length(3), amount: z.string(), bankAccount: z.string().optional() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid payment."); return Response.json({ data: await createPayment(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "payments" && parts[2] === "approve") { const parsed = z.object({ expectedVersion: z.number().int().positive(), proof: z.string() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Payment approval evidence is required."); return Response.json({ data: await approvePayment(actor, parts[1], parsed.data, needCommand(request)) }); }
    if (parts[0] === "payments" && parts[2] === "release") { const parsed = z.object({ expectedVersion: z.number().int().positive(), proof: z.string() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Payment release evidence is required."); return Response.json({ data: await releasePayment(actor, parts[1], parsed.data, needCommand(request)) }); }
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
    if (parts[0] === "bank" && parts[1] === "statements" && parts[2] === "import") { const parsed = z.object({ bankAccount: z.string(), statementNumber: z.string(), statementDate: z.string().date(), opening: z.string(), closing: z.string(), sourceHash: z.string().min(8), rows: z.array(z.object({ externalId: z.string(), transactionDate: z.string().date(), description: z.string().optional(), amount: z.string(), currency: z.string().length(3) })).min(1) }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid bank statement."); return Response.json({ data: await importBankStatement(actor, parsed.data, needCommand(request)) }, { status: 201 }); }
    if (parts[0] === "bank" && parts[1] === "transactions" && parts[3] === "reconcile") { const parsed = z.object({ expectedVersion: z.number().int().positive(), matchedType: z.string(), matchedId: z.string(), proof: z.string() }).safeParse(input); if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Reconciliation evidence is required."); return Response.json({ data: await reconcileBankTransaction(actor, parts[2], parsed.data, needCommand(request)) }); }
    throw new ApiError(404, "NOT_FOUND", "Finance resource not found.");
  } catch (error) { return errorResponse(error); }
}

async function listPeriods(actor: import("@/server/authorization").WorkContext) {
  const { holdingResources } = await import("@/server/runtime");
  const { write } = await holdingResources(actor.holdingId);
  return (await write.prepare("SELECT * FROM finance_periods WHERE company_id=? ORDER BY start_date DESC").bind(actor.companyId).all()).results;
}
