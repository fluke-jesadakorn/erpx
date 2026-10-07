import { describe, expect, it, vi } from "vitest";
import type { WorkContext } from "../../src/server/modules/identity_access/public/authorization";
import {
  createSaveExpenseDraft, createSaveExpenseAccounting, createPostExpense,
  createExpensePaymentCommands, createTransitionExpenseV2,
  type ExpenseAccountingPorts, type ExpenseDraftPorts, type ExpensePaymentPorts,
  type ExpensePostingPorts, type ExpenseTransitionPorts, type ExpenseCommandPort,
} from "../../src/server/modules/people_payroll/application/expense-commands";
import { createExpenseAiStatus, type ExpenseAiStatusPorts } from "../../src/server/modules/people_payroll/application/expense-ai-status";
import { hasIndependentSpendingTeam, independentAccountingCandidates, selectSpendingException } from "../../src/server/modules/people_payroll/domain/expense-routing-rules";
import { createExpenseRequirements, createSaveExpensePayment, createRevokePaymentSlip, createRevokeExpenseAttachment, type ExpensePaymentPreparationPorts, type ExpensePaymentSlipRemovalPorts, type ExpenseReceiptRemovalPorts } from "../../src/server/modules/people_payroll/application/expense-evidence";
import { createExpenseReads, type ExpenseReadPorts } from "../../src/server/modules/people_payroll/application/expense-reads";
import { createTransitionExpense, createPrepareExpensePosting, createReverseExpense, type ExpenseLifecyclePorts, type ExpensePostingPreparationPorts, type ExpenseReversalPorts } from "../../src/server/modules/people_payroll/application/expense-lifecycle";
import { createQueueExpenseAi, createRunExpenseAi, type ExpenseAiQueuePorts, type ExpenseAiWorkerPorts, type ExpenseAiSourceFacts } from "../../src/server/modules/people_payroll/application/expense-ai-jobs";
import type { AiClaim } from "../../src/server/modules/people_payroll/public/expense-ai-source";
import { reconcileExpenseAiResults } from "../../src/server/modules/people_payroll/domain/expense-ai-processing";
import { createExpenseRoute, createLegacyExpenseRoute, createSpendingUsers, createSpendingStepCandidates, createExpensePoolCandidates, type ExpenseRoutingPorts } from "../../src/server/modules/people_payroll/application/expense-routing";
import { createExpensePayers, createExpenseWorkflowFields, createCurrentExpenseReader, createExpenseApprovalFields, createExpenseDetail, createExpenseOptions, createExpensePaymentReader } from "../../src/server/modules/people_payroll/application/expense-projections";
import { createExpenseAiAllowed, createExpenseAiSource, createExpenseAiClaim, type ExpenseAiEligibilityPorts, type ExpenseAiSourcePorts } from "../../src/server/modules/people_payroll/application/expense-ai-source";
import type { OrganizationPerson } from "../../src/server/modules/organization/public/profile-facts";
import type { StaffPerson } from "../../src/server/modules/operations/public/staff-policy";
import type { ApprovalInstance, ApprovalStep } from "../../src/server/modules/workflow/public/approvals";

const actor = { userId: "claimant", name: "Claimant", companyId: "company", holdingId: "h001", branchId: null, departmentId: null } as WorkContext;
const row = { id: "expense", company_id: "company", branch_id: "branch", department_id: "department", claimant_user_id: "claimant", created_by: "claimant", prepared_by: "claimant", workflow_version: 2, workflow_stage: "accounts_review", status: "in_review", amount_minor: 10000, tax_minor: 700, expense_date: "2026-09-01", version: 4 };
const mutation = { result: { id: "expense", version: 5 }, native: { fixture: "opaque-native-work" } };
function common(): ExpenseCommandPort {
  return {
    replay: vi.fn(async () => null),
    commit: vi.fn(async (_actor, _key, _operation, _input, _id, _version, work) => work.result),
    error: violation => Object.assign(new Error(violation.message), violation),
  };
}
function drafts(): ExpenseDraftPorts {
  return { ...common(), expense: vi.fn(async () => row), authority: vi.fn(async () => "authority"), claimant: vi.fn(async () => null), staff: vi.fn(async () => null), latestEngagement: vi.fn(async () => null), canManagePeople: vi.fn(() => false), money: value => Number(value) * 100, prepareDraft: vi.fn(async () => mutation) };
}
function accounting(): ExpenseAccountingPorts {
  return { ...common(), authority: vi.fn(async () => "authority"), expense: vi.fn(async () => row), approval: vi.fn(async () => ({ canDecide: true, instance: { id: "approval" } })), money: value => Number(value) * 100, prepareAccounting: vi.fn(async () => mutation) };
}
function payments(): ExpensePaymentPorts {
  return { ...common(), expense: vi.fn(async () => ({ ...row, workflow_stage: "payment" })), payers: vi.fn(async () => [actor.userId]), preparePayment: vi.fn(async () => mutation), payment: vi.fn(async () => ({ expense: row, draft: { id: "draft", version: 2 } })), payable: vi.fn(async () => ({ id: "invoice" })), paymentEvidence: vi.fn(async () => "bound-evidence"), hasPaymentSlip: vi.fn(async () => false), prepareConfirmation: vi.fn(async () => mutation) };
}

describe("Expense application commands with injected semantic ports", () => {
  it("replays a draft without capturing authority or preparing another mutation", async () => {
    const ports = drafts(), prior = { id: "expense-same-key", version: 1 };
    vi.mocked(ports.replay).mockResolvedValue(prior);
    expect(await createSaveExpenseDraft(ports)(actor, null, { expectedVersion: 1 }, "same-key")).toBe(prior);
    expect(ports.authority).not.toHaveBeenCalled();
    expect(ports.prepareDraft).not.toHaveBeenCalled();
    expect(ports.commit).not.toHaveBeenCalled();
  });
  it("preserves an incomplete member draft without requiring a People engagement", async () => {
    const ports = drafts();
    await createSaveExpenseDraft(ports)(actor, null, { expectedVersion: 1 }, "draft-key");
    expect(ports.prepareDraft).toHaveBeenCalledWith(actor, "draft-key", { expectedVersion: 1 }, expect.objectContaining({ recordId: "expense-draft-key", claimant: "claimant", staff: null, amount: null, requestKind: "expense" }), "authority");
    expect(ports.commit).toHaveBeenCalledWith(actor, "draft-key", "expense.create", { expectedVersion: 1 }, "expense-draft-key", 1, mutation, { companyId: "company", branchId: null, departmentId: null });
  });
  it("denies a claimant change on an existing editable draft", async () => {
    const ports = drafts();
    vi.mocked(ports.expense).mockResolvedValue({ ...row, status: "draft" });
    vi.mocked(ports.claimant).mockResolvedValue({ id: "person-other", user_id: "other", legal_name: "Other" });
    await expect(createSaveExpenseDraft(ports)(actor, "expense", { expectedVersion: 4, personId: "person-other" }, "change-key")).rejects.toMatchObject({ code: "CLAIMANT_IMMUTABLE" });
    expect(ports.prepareDraft).not.toHaveBeenCalled();
  });
  it("rejects invalid accounting before native mutation preparation", async () => {
    const ports = accounting();
    await expect(createSaveExpenseAccounting(ports)(actor, "expense", { expectedVersion: 4, categoryAccountId: "account", tax: "107", entryDate: "2026-09-02", periodId: "period", note: "" }, "accounting-key")).rejects.toMatchObject({ code: "TAX_INVALID" });
    expect(ports.prepareAccounting).not.toHaveBeenCalled();
    expect(ports.commit).not.toHaveBeenCalled();
  });
  it("preserves gross money and commits accounting with the Expense record scope", async () => {
    const ports = accounting(), input = { expectedVersion: 4, categoryAccountId: "account", tax: "7", entryDate: "2026-09-02", periodId: "period", note: "Reviewed" };
    await createSaveExpenseAccounting(ports)(actor, "expense", input, "accounting-key");
    expect(ports.prepareAccounting).toHaveBeenCalledWith(actor, "accounting-key", "expense", input, row, { canDecide: true, instance: { id: "approval" } }, "authority", 700, 10000);
    expect(ports.commit).toHaveBeenCalledWith(actor, "accounting-key", "expense.accounting.save", input, "expense", 5, mutation, { companyId: "company", branchId: "branch", departmentId: "department" });
  });
  it("denies self posting before touching Finance preparation", async () => {
    const ports: ExpensePostingPorts = { ...common(), expense: vi.fn(async () => ({ ...row, status: "approved", workflow_stage: "gl_posting", can_post: 1, accounting_json: JSON.stringify({ preparedBy: actor.userId, periodId: "period", entryDate: "2026-09-02" }) })), legacyPoster: vi.fn(value => value), preparePosting: vi.fn(async () => mutation) };
    await expect(createPostExpense(ports)(actor, "expense", { expectedVersion: 4, periodId: "period", entryDate: "2026-09-02" }, "post-key")).rejects.toMatchObject({ code: "SELF_POST_FORBIDDEN" });
    expect(ports.preparePosting).not.toHaveBeenCalled();
    expect(ports.commit).not.toHaveBeenCalled();
  });
  it("requires an independent eligible Treasury person", async () => {
    const ports = payments();
    vi.mocked(ports.payers).mockResolvedValue(["another-treasury"]);
    await expect(createExpensePaymentCommands(ports).createExpensePayment(actor, "expense", 4, "payment-key")).rejects.toMatchObject({ code: "TREASURY_REQUIRED" });
    expect(ports.preparePayment).not.toHaveBeenCalled();
  });
  it("rejects payment evidence belonging to another claim before reading its payable", async () => {
    const ports = payments();
    await expect(createExpensePaymentCommands(ports).confirmExpensePayment(actor, "draft", { expectedVersion: 2, date: "2026-09-02", amount: "107", reference: "bank-ref" }, "payment-key", "foreign-expense")).rejects.toMatchObject({ code: "PAYMENT_DRAFT_NOT_FOUND" });
    expect(ports.payable).not.toHaveBeenCalled();
    expect(ports.commit).not.toHaveBeenCalled();
  });
  it("commits settlement and slip evidence as one opaque mutation", async () => {
    const ports = payments(), input = { expectedVersion: 2, date: "2026-09-02", amount: "50", reference: "bank-ref" };
    await createExpensePaymentCommands(ports).confirmExpensePayment(actor, "draft", input, "payment-key", "expense");
    expect(ports.hasPaymentSlip).not.toHaveBeenCalled();
    expect(ports.prepareConfirmation).toHaveBeenCalledWith(actor, "payment-key", "draft", input, row, { id: "draft", version: 2 }, { id: "invoice" }, "bound-evidence");
    expect(ports.commit).toHaveBeenCalledTimes(1);
    expect(ports.commit).toHaveBeenCalledWith(actor, "payment-key", "expense.payment.confirm", input, "draft", 3, mutation, expect.objectContaining({ companyId: "company" }));
  });
  it("does not open an approval when submission setup is incomplete", async () => {
    const ports: ExpenseTransitionPorts = {
      ...common(), now: () => 123, requirements: vi.fn(async () => ["Attach the receipt."]),
      staff: vi.fn(), route: vi.fn(), open: vi.fn(), decision: vi.fn(), prepareSubmit: vi.fn(), prepareCancel: vi.fn(), prepareGlReturn: vi.fn(), prepareAccountingApproval: vi.fn(), prepareDecision: vi.fn(),
    };
    await expect(createTransitionExpenseV2(ports)(actor, { ...row, status: "draft" }, "submit", { expectedVersion: 4 }, "submit-key")).rejects.toMatchObject({ code: "EXPENSE_SETUP_REQUIRED" });
    expect(ports.open).not.toHaveBeenCalled();
    expect(ports.commit).not.toHaveBeenCalled();
  });
});

describe("Expense routing rules", () => {
  it("requires three different people after excluding the department approver", () => {
    expect(hasIndependentSpendingTeam(["a"], ["b"], ["a", "b"])).toBe(false);
    expect(hasIndependentSpendingTeam(["a"], ["b"], ["c"])).toBe(true);
    expect(hasIndependentSpendingTeam(["a"], ["b"], ["c"], ["c"])).toBe(false);
  });
  it("keeps only candidates with an independent subsequent reviewer and payer", () => {
    expect(independentAccountingCandidates("accounts_review", ["a", "b"], ["b"], ["c"])).toEqual(["a"]);
    expect(independentAccountingCandidates("accounts_approval", ["b", "c"], [], ["c"])).toEqual(["b"]);
  });
  it("prioritizes matching position exceptions and keeps equal-specificity input order", () => {
    const base = { action: "skip_supervisor" as const, supervisorUserId: null, reason: "approved policy" };
    const company = { ...base, positionId: null, requestKind: null };
    const kind = { ...base, positionId: null, requestKind: "expense" as const };
    const position = { ...base, positionId: "position", requestKind: null };
    const exact = { ...base, positionId: "position", requestKind: "expense" as const };
    expect(selectSpendingException([company, kind, position, exact], "position", "expense")).toBe(exact);
    expect(selectSpendingException([company, kind, position], "position", "expense")).toBe(position);
    expect(selectSpendingException([exact, { ...exact, reason: "later" }], "position", "expense")).toBe(exact);
  });
});

describe("Expense AI status with injected sources and provider configuration", () => {
  const proposal = { id: "proposal", state: "ready", stage: "accounts_review", source_version: 4, evidence_digest: "digest", provider_json: "provider-snapshot", finance_policy_version: 7, result_json: null, error: null };
  function aiPorts(): ExpenseAiStatusPorts<WorkContext, { workflow_stage: string }> {
    return {
      source: vi.fn(async () => ({ claim: { workflow_stage: "accounts_review" }, sourceVersion: 4, digest: "digest", evidence: [] })),
      allowed: vi.fn(async () => true), proposals: vi.fn(async () => [proposal]),
      providerConfiguration: vi.fn(async () => ({ snapshot: "provider-snapshot", unavailable: null })),
      financePolicyVersion: vi.fn(async () => 7),
    };
  }
  it("marks a suggestion stale when Finance policy changes", async () => {
    const ports = aiPorts();
    vi.mocked(ports.financePolicyVersion).mockResolvedValue(8);
    expect(await createExpenseAiStatus(ports)(actor, "expense", null)).toMatchObject({ proposal: null, stale: true, sourceVersion: 4, evidenceDigest: "digest" });
  });
  it("reports provider unavailability without returning an old provider's suggestion", async () => {
    const ports = aiPorts();
    vi.mocked(ports.providerConfiguration).mockResolvedValue({ snapshot: null, unavailable: "Provider retired" });
    expect(await createExpenseAiStatus(ports)(actor, "expense", null)).toMatchObject({ unavailable: "Provider retired", proposal: null, stale: true });
  });
  it("requires every PDF page and isolates payment evidence", async () => {
    const ports = aiPorts();
    const receipt = { id: "receipt", name: "receipt.pdf", type: "application/pdf", purpose: "expense_receipt", checksum: "receipt-checksum", pageCount: 2, pages: [{}] };
    const slip = { id: "slip", name: "slip.png", type: "image/png", purpose: "expense_payment", checksum: "slip-checksum", pageCount: null, pages: [] };
    vi.mocked(ports.source).mockResolvedValue({ claim: { workflow_stage: "accounts_review" }, sourceVersion: 4, digest: "digest", evidence: [receipt, slip] });
    expect(await createExpenseAiStatus(ports)(actor, "expense", "payment")).toMatchObject({ hasEvidence: true, incomplete: [{ id: "slip", name: "slip.png", type: "image/png", checksum: "slip-checksum" }], proposal: { id: "proposal", state: "ready", result: null, error: null }, stale: false });
    expect(await createExpenseAiStatus(ports)(actor, "expense", null)).toMatchObject({ incomplete: [{ id: "receipt" }, { id: "slip" }] });
  });
});

describe("Expense supporting evidence and payment preparation", () => {
  it("retains setup message ordering and the payment-request supporting-document rule", async () => {
    const requirements = createExpenseRequirements({ employment: async () => ({ unavailable: "People profile unavailable" }), hasReceipt: async () => false });
    expect(await requirements({ id: "request", request_kind: "payment" })).toEqual([
      "People profile unavailable", "Add the expense date.", "Add the merchant.", "Add the receipt number.", "Explain the business purpose.", "Add a positive amount and currency.", "Choose an expense category.", "Attach the payment request’s supporting document before submitting.",
    ]);
  });
  it("evaluates both active-level and engagement-date requirements before receipt availability", async () => {
    const requirements = createExpenseRequirements({ employment: async () => ({ startDate: "2026-09-02", endDate: null, activeLevel: false }), hasReceipt: async () => true });
    expect(await requirements({ ...row, merchant: "Merchant", receipt_reference: "Receipt", business_purpose: "Business expense", currency: "THB", category_account_id: "account" })).toEqual(["Ask an administrator to set your active staff level in People.", "The expense date must fall within your engagement dates."]);
  });
  it("rejects overpayment before binding evidence or preparing a native write", async () => {
    const ports: ExpensePaymentPreparationPorts = { ...common(), payment: async () => ({ expense: { ...row, outstanding_minor: 5000 }, draft: { version: 2 } }), money: value => Number(value) * 100, evidence: vi.fn(), prepareSaving: vi.fn() };
    await expect(createSaveExpensePayment(ports)(actor, "draft", { expectedVersion: 2, date: "2026-09-02", amount: "51", reference: "ref" }, "save-key", "expense")).rejects.toMatchObject({ code: "PAYMENT_AMOUNT_INVALID" });
    expect(ports.evidence).not.toHaveBeenCalled();
    expect(ports.prepareSaving).not.toHaveBeenCalled();
  });
  it("replays slip removal even when current evidence storage is read-only", async () => {
    const ports: ExpensePaymentSlipRemovalPorts = { ...common(), readOnly: vi.fn(() => true), payment: vi.fn(), link: vi.fn(), prepareRemoval: vi.fn() };
    const prior = { id: "draft", version: 3 };
    vi.mocked(ports.replay).mockResolvedValue(prior);
    expect(await createRevokePaymentSlip(ports)(actor, "expense", "draft", "slip", 2, "same-key")).toBe(prior);
    expect(ports.readOnly).not.toHaveBeenCalled();
    expect(ports.payment).not.toHaveBeenCalled();
  });
  it("preserves immutable bound receipt evidence before planning withdrawal", async () => {
    const ports: ExpenseReceiptRemovalPorts = { ...common(), readOnly: () => false, expense: async () => ({ ...row, status: "draft" }), link: async () => ({ id: "link", state: "bound", set_id: "set" }), draftLink: vi.fn(), prepareRemoval: vi.fn() };
    await expect(createRevokeExpenseAttachment(ports)(actor, "expense", "receipt", 4, "remove-key")).rejects.toMatchObject({ code: "BOUND_ATTACHMENT_IMMUTABLE" });
    expect(ports.draftLink).not.toHaveBeenCalled();
    expect(ports.prepareRemoval).not.toHaveBeenCalled();
  });
});

describe("Expense owner reads", () => {
  function reads(): ExpenseReadPorts<typeof row> {
    return { legacyIds: async () => ["legacy"], isLegacy: async () => false, initializeApproval: vi.fn(async () => {}), visibleRows: async () => [row, { ...row, id: "hidden" }], visibleRow: async () => row, canRead: async (_actor, value) => value.id !== "hidden", decorate: vi.fn(async (_actor, value) => value), error: common().error };
  }
  it("initializes legacy approvals and never decorates records denied by current IAM", async () => {
    const ports = reads();
    expect(await createExpenseReads(ports).list(actor)).toEqual([row]);
    expect(ports.initializeApproval).toHaveBeenCalledWith(actor, "legacy");
    expect(ports.decorate).toHaveBeenCalledTimes(1);
    expect(ports.decorate).toHaveBeenCalledWith(actor, row);
  });
  it("uses the same not-found response for a stale reader and a missing visible record", async () => {
    const ports = reads();
    ports.canRead = async () => false;
    await expect(createExpenseReads(ports).get(actor, "expense")).rejects.toMatchObject({ status: 404, code: "EXPENSE_NOT_FOUND" });
    ports.visibleRow = async () => null;
    await expect(createExpenseReads(ports).get(actor, "missing")).rejects.toMatchObject({ status: 404, code: "EXPENSE_NOT_FOUND" });
    expect(ports.decorate).not.toHaveBeenCalled();
  });
});

describe("Expense legacy lifecycle and journal orchestration", () => {
  const accountant = { ...actor, userId: "accountant", role: "finance_approver" } as WorkContext;
  it("sends every submission to the current workflow without recreating a legacy chain", async () => {
    const ports: ExpenseLifecyclePorts = { ...common(), expense: async () => ({ ...row, workflow_version: 1, status: "draft" }), current: vi.fn(async () => ({ id: "expense", status: "in_review" })), canOverride: vi.fn(), decision: vi.fn(), prepareLegacy: vi.fn() };
    await createTransitionExpense(ports)(actor, "expense", "submit", { expectedVersion: 4 }, "submit-key");
    expect(ports.current).toHaveBeenCalled();
    expect(ports.decision).not.toHaveBeenCalled();
    expect(ports.prepareLegacy).not.toHaveBeenCalled();
  });
  it("retains the saved legacy approval decision and final status", async () => {
    const reviewer = { ...actor, userId: "reviewer" }, legacy = { ...row, workflow_version: 1, current_approver_id: "reviewer", can_decide: 1 };
    const decision = { final: true, native: "existing-approval-cycle" };
    const ports: ExpenseLifecyclePorts = { ...common(), expense: async () => legacy, current: vi.fn(), canOverride: vi.fn(), decision: vi.fn(async () => decision), prepareLegacy: vi.fn(async () => mutation) };
    await createTransitionExpense(ports)(reviewer, "expense", "approve", { expectedVersion: 4, reason: "Reviewed" }, "approve-key");
    expect(ports.prepareLegacy).toHaveBeenCalledWith(reviewer, "expense", legacy, "approve", { expectedVersion: 4, reason: "Reviewed" }, "approve-key", decision, null, null, "approved");
    expect(ports.current).not.toHaveBeenCalled();
  });
  it("builds balanced expense, payable and recoverable-tax journal lines", async () => {
    const config = { chart_id: "chart", payable_account_id: "payable", input_tax_account_id: "tax" }, journal = { id: "journal", native: "owned-journal" };
    const ports: ExpensePostingPreparationPorts = { error: common().error, mapping: async () => config, journal: vi.fn(async () => journal), preparePosting: vi.fn(async () => ({ result: { id: "expense", status: "posted", version: 5, invoiceId: "invoice", journalId: "journal" }, native: "native-posting" })) };
    const claim = { ...row, category_account_id: "expense-account", currency: "THB", employee_id: "person", legal_name: "Claimant", merchant: "Merchant" };
    await createPrepareExpensePosting(ports)(accountant, claim, { expectedVersion: 4, periodId: "period", entryDate: "2026-09-02" }, "post-key");
    expect(ports.journal).toHaveBeenCalledWith(accountant, expect.objectContaining({ lines: [{ accountId: "expense-account", debit: "100.00", credit: "0" }, { accountId: "payable", debit: "0", credit: "107.00" }, { accountId: "tax", debit: "7.00", credit: "0" }] }), "post-key");
    expect(ports.preparePosting).toHaveBeenCalledWith(accountant, claim, expect.anything(), "post-key", config, 10700, "expense-invoice-expense", "expense-payee-person", journal);
  });
  it("rejects settled expense reversal before reading or changing its journal", async () => {
    const ports: ExpenseReversalPorts = { ...common(), authorize: value => value, expense: async () => ({ ...row, status: "posted" }), invoice: async () => ({ status: "posted", total_minor: 10700, outstanding_minor: 10000 }), journal: vi.fn(), prepareReversal: vi.fn() };
    await expect(createReverseExpense(ports)(accountant, "expense", { expectedVersion: 4, periodId: "period", entryDate: "2026-09-02", reason: "Correction" }, "reverse-key")).rejects.toMatchObject({ code: "EXPENSE_HAS_SETTLEMENT" });
    expect(ports.journal).not.toHaveBeenCalled();
    expect(ports.prepareReversal).not.toHaveBeenCalled();
  });
});

describe("Expense AI queue and lease-owned worker orchestration", () => {
  const aiActor = { ...actor, role: "member", assignmentId: "assignment" };
  const evidence = { id: "receipt", name: "receipt.pdf", type: "application/pdf", purpose: "expense_claim", key: "receipt-key", checksum: "checksum", bytes: 1000, pageCount: 3, pages: [1, 2, 3].map(page => ({ page, key: `page-${page}`, checksum: `checksum-${page}`, bytes: 100 })) };
  const source: ExpenseAiSourceFacts = { claim: { ...row, workflow_stage: "draft", accounting_json: "{}" } as AiClaim, payment: null, evidence: [evidence], sourceVersion: 4, digest: "digest", native: "source-guards" };
  function queue(): ExpenseAiQueuePorts {
    return { ...common(), captureActor: vi.fn(async value => value), financePolicy: async () => null, enabled: vi.fn(async () => {}), authority: async () => "authority", source: async () => source, allowed: async () => true, configuration: async () => ({ snapshot: "{}", native: "providers" }), existing: async () => null, now: () => 123, month: () => "2026-09", quota: async () => ({ used: 1, allowance: 100, busy: 0 }), prepareQueue: vi.fn(async () => mutation) };
  }
  function worker(): ExpenseAiWorkerPorts & { stopped: ReturnType<typeof vi.fn> } {
    const stopped = vi.fn(async () => {}), row = { id: "job", company_id: "company", expense_id: "expense", payment_draft_id: null, stage: "draft", evidence_digest: "digest", provider_json: "{}", actor_json: JSON.stringify(aiActor), finance_policy_version: null };
    return {
      stopped, error: common().error, claim: async () => ({ lease: "lease", row }), renewal: () => stopped, enabled: async () => {}, authorize: () => {}, financePolicy: async () => null, authority: async () => "authority", source: async () => source,
      configuration: vi.fn(async () => ({ snapshot: "{}", native: "providers" })), allowed: async () => true, accounts: async () => [], periods: async () => [], renew: vi.fn(async () => {}), imageContent: vi.fn(async () => "opaque-image-content"),
      extract: vi.fn(async () => ({ result: { summary: "Prepared", fields: [], missing: [], conflicts: [] }, inputTokens: 2, outputTokens: 3 })), consolidate: vi.fn(async () => ({ result: { summary: "Consolidated", fields: [], missing: [], conflicts: [] }, inputTokens: 4, outputTokens: 5 })), complete: vi.fn(async () => {}),
      errorFacts: error => { const failure = error as { code?: string; message?: string }; return { code: failure.code ?? null, message: failure.code ? failure.message ?? null : null, constraint: /constraint/i.test(String(error)) }; }, holdsLease: async () => true, fail: vi.fn(async () => {}),
    };
  }
  it("captures job authority before replay and skips policy/quota preparation on replay", async () => {
    const ports = queue(), prior = { id: "expense", proposalId: "old", state: "queued" };
    vi.mocked(ports.replay).mockResolvedValue(prior);
    expect(await createQueueExpenseAi(ports)(aiActor, "expense", { expectedVersion: 4, paymentId: null, retry: false }, "same-key")).toBe(prior);
    expect(ports.captureActor).toHaveBeenCalled();
    expect(ports.enabled).not.toHaveBeenCalled();
    expect(ports.prepareQueue).not.toHaveBeenCalled();
  });
  it("returns an existing running preparation even for a retry request", async () => {
    const ports = queue();
    ports.existing = async () => ({ id: "running-job", state: "running" });
    expect(await createQueueExpenseAi(ports)(aiActor, "expense", { expectedVersion: 4, paymentId: null, retry: true }, "retry-key")).toEqual({ id: "expense", proposalId: "running-job", state: "running" });
    expect(ports.prepareQueue).not.toHaveBeenCalled();
  });
  it("enforces the company concurrency quota before preparing a transaction", async () => {
    const ports = queue();
    ports.quota = async () => ({ used: 1, allowance: 100, busy: 2 });
    await expect(createQueueExpenseAi(ports)(aiActor, "expense", { expectedVersion: 4, paymentId: null, retry: false }, "queue-key")).rejects.toMatchObject({ code: "AI_BUSY" });
    expect(ports.prepareQueue).not.toHaveBeenCalled();
  });
  it("passes a ready proposal retry into the same native quota-guarded queue mutation", async () => {
    const ports = queue(), existing = { id: "ready-job", state: "ready" }, input = { expectedVersion: 4, paymentId: null, retry: true };
    ports.existing = async () => existing;
    await createQueueExpenseAi(ports)(aiActor, "expense", input, "retry-key");
    expect(ports.prepareQueue).toHaveBeenCalledWith(aiActor, "expense", input, "retry-key", source, { snapshot: "{}", native: "providers" }, null, "authority", existing, 123, "2026-09");
    expect(ports.commit).toHaveBeenCalledTimes(1);
  });
  it("processes two evidence pages per image call, consolidates, and records exact usage", async () => {
    const ports = worker();
    await createRunExpenseAi(ports)("job");
    expect(ports.imageContent).toHaveBeenNthCalledWith(1, expect.anything(), expect.arrayContaining([expect.objectContaining({ page: 1 }), expect.objectContaining({ page: 2 })]));
    expect(ports.imageContent).toHaveBeenNthCalledWith(2, expect.anything(), [expect.objectContaining({ page: 3 })]);
    expect(ports.extract).toHaveBeenCalledTimes(2);
    expect(ports.consolidate).toHaveBeenCalledTimes(1);
    expect(ports.complete).toHaveBeenCalledWith("job", "lease", expect.anything(), expect.anything(), "authority", source, expect.anything(), 8, 11, { image: { input: 4, output: 6, attempted: true }, text: { input: 4, output: 5, attempted: true } });
    expect(ports.fail).not.toHaveBeenCalled();
    expect(ports.stopped).toHaveBeenCalledTimes(1);
  });
  it("marks provider changes stale before attempting generation and always stops renewal", async () => {
    const ports = worker();
    vi.mocked(ports.configuration).mockResolvedValueOnce({ snapshot: "{}", native: "providers" }).mockResolvedValue({ snapshot: "changed", native: "new-provider" });
    await createRunExpenseAi(ports)("job");
    expect(ports.extract).not.toHaveBeenCalled();
    expect(ports.fail).toHaveBeenCalledWith("job", "lease", expect.anything(), expect.anything(), true, "The source, authority, or provider changed. Prepare new suggestions.", 0, 0, { image: { input: 0, output: 0, attempted: false }, text: { input: 0, output: 0, attempted: false } });
    expect(ports.stopped).toHaveBeenCalledTimes(1);
  });
  it("does not overwrite another worker after losing its lease", async () => {
    const ports = worker();
    ports.imageContent = async () => { throw new Error("file read failed"); };
    ports.holdsLease = async () => false;
    await createRunExpenseAi(ports)("job");
    expect(ports.complete).not.toHaveBeenCalled();
    expect(ports.fail).not.toHaveBeenCalled();
    expect(ports.stopped).toHaveBeenCalledTimes(1);
  });
  it("records provider usage attempts on failure and skips consolidation", async () => {
    const ports = worker();
    ports.extract = async () => { throw new Error("provider failed"); };
    await createRunExpenseAi(ports)("job");
    expect(ports.consolidate).not.toHaveBeenCalled();
    expect(ports.fail).toHaveBeenCalledWith("job", "lease", expect.anything(), expect.anything(), false, expect.stringContaining("Retry"), 0, 0, { image: { input: 0, output: 0, attempted: true }, text: { input: 0, output: 0, attempted: false } });
    expect(ports.stopped).toHaveBeenCalledTimes(1);
  });
  it("keeps conflicting evidence values null after consolidation", () => {
    const first = { summary: "First", fields: [{ name: "merchant" as const, value: "Merchant A", citations: [{ attachmentId: "receipt", page: 1 }] }], missing: [], conflicts: [] };
    const second = { ...first, fields: [{ ...first.fields[0], value: "Merchant B", citations: [{ attachmentId: "receipt", page: 3 }] }] };
    const consolidated = { ...first, fields: first.fields.map(field => ({ ...field })), conflicts: [] };
    expect(reconcileExpenseAiResults(consolidated, [first, second], "draft", 0)).toMatchObject({ fields: [{ name: "merchant", value: null }], conflicts: ["Conflicting evidence for merchant; review the cited pages manually."] });
  });
});

function profile(user: string, changes: Partial<OrganizationPerson> = {}): OrganizationPerson {
  return { person_id: `person-${user}`, user_id: user, legal_name: user, email: null, engagement_id: `engagement-${user}`, engagement_version: 1, person_version: 1, company_id: "company", branch_id: "branch", department_id: "department", position_id: "position", manager_person_id: "person-head", level_id: "level", level_number: 4, level_name: "Supervisor", status: "active", person_status: "active", start_date: "2026-01-01", end_date: null, job_title: "staff", issues: [], ...changes };
}
const staff = { ...profile("claimant", { level_number: 6 }), user_id: "claimant", worker_type: "employee", level_rank: 1 } as StaffPerson;
function routing(): ExpenseRoutingPorts {
  const profiles = [profile("claimant", { level_number: 6 }), profile("head", { level_number: 3 })];
  return {
    error: common().error, rejection: error => error instanceof Error && "code" in error ? error.message : null,
    authority: vi.fn(async () => ({ native: "authority-guard" })), staff: vi.fn(async () => staff), activeDepartment: vi.fn(async () => true), levels: vi.fn(async () => [{ id: "level", rank: 1 }]),
    staffPeople: vi.fn(async () => [staff]), reportingChain: vi.fn(() => []), engagementGuard: vi.fn(() => ({ native: "engagement-guard" })),
    roleUsers: vi.fn(async (_scope, role) => role === null ? ["claimant", "head"] : role === "holding_owner" ? ["head"] : role === "finance_preparer" ? ["preparer"] : role === "finance_approver" ? ["reviewer"] : ["payer"]),
    identities: vi.fn(async (_company, users) => users), roster: vi.fn(async () => ({ people: profiles, issues: () => [] })), today: () => "2026-10-05",
    template: vi.fn(async () => ({ version: 7, reason: "policy", exceptions: [] })),
    department: vi.fn(async () => ({ scope: staff, blockedReason: null, steps: [{ userId: "head", authority: "named" as const, candidateUserIds: ["head"], stage: "department_approval", snapshot: { departmentPolicyVersion: 2 } }], guards: [{ native: "department-guard" }] })),
    departmentEligible: vi.fn(async () => true), spendingUsers: vi.fn(async (_scope, role) => [profile(role === "finance_preparer" ? "preparer" : role === "finance_approver" ? "reviewer" : "payer")]),
    claim: vi.fn(async () => ({ claimant_user_id: "claimant", prepared_by: "claimant", accounting_json: "{}" })),
  };
}

describe("Expense profile and saved candidate routing through owner facts", () => {
  it("never permits a missing-supervisor exception to bypass requester access", async () => {
    const ports = routing();
    vi.mocked(ports.roster).mockResolvedValue({ people: [profile("claimant", { level_number: 6 })], issues: () => ["Assign a reporting manager."] });
    vi.mocked(ports.template).mockResolvedValue({ version: 7, reason: "approved policy", exceptions: [{ positionId: "position", requestKind: "expense", action: "skip_supervisor", supervisorUserId: null, reason: "CEO exception" }] });
    vi.mocked(ports.roleUsers).mockResolvedValue([]);
    expect(await createExpenseRoute(ports)("company", "claimant")).toMatchObject({ steps: [], blockedReason: "The requester’s account must cover this branch and department." });
    expect(ports.spendingUsers).not.toHaveBeenCalled();
  });
  it("captures independent finance pools and both owner authority guards", async () => {
    const ports = routing();
    const route = await createExpenseRoute(ports)("company", "claimant", ["assistant"], false, "purchase");
    expect(route.steps.map(step => step.userId)).toEqual(["head", "preparer", "reviewer"]);
    expect(route.guards).toEqual([{ native: "authority-guard" }, { native: "department-guard" }]);
    expect(route.steps[0].snapshot).toMatchObject({ spendingPolicy: 1, templateVersion: 7, requestKind: "purchase", excluded: ["assistant"], departmentPolicyVersion: 2 });
  });
  it("keeps the legacy owner fallback and requires four distinct participants", async () => {
    const ports = routing();
    expect((await createLegacyExpenseRoute(ports)("company", "claimant")).steps[0]).toMatchObject({ userId: "head", authority: "holding_owner" });
    vi.mocked(ports.roleUsers).mockImplementation(async (_scope, role) => role === null ? ["claimant"] : role === "holding_owner" ? ["head"] : role === "finance_preparer" ? ["preparer"] : ["reviewer"]);
    expect(await createLegacyExpenseRoute(ports)("company", "claimant")).toMatchObject({ steps: [], blockedReason: expect.stringContaining("Four different people") });
  });
  it("propagates infrastructure failure instead of hiding it as route setup", async () => {
    const ports = routing(), unavailable = new Error("owner unavailable");
    vi.mocked(ports.authority).mockRejectedValue(unavailable);
    await expect(createExpenseRoute(ports)("company", "claimant")).rejects.toBe(unavailable);
  });
  it("filters inactive employment, reporting issues, unmapped accounts and Level 5 approvers", async () => {
    const ports = routing(), people = [profile("eligible"), profile("future", { start_date: "2027-01-01" }), profile("ended", { end_date: "2026-01-02" }), profile("inactive", { person_status: "inactive" }), profile("broken"), profile("senior", { level_number: 5 }), profile("unmapped")];
    vi.mocked(ports.roster).mockResolvedValue({ people, issues: person => person.user_id === "broken" ? ["Assign a reporting manager."] : [] });
    vi.mocked(ports.roleUsers).mockResolvedValue(people.filter(person => person.user_id !== "unmapped").map(person => person.user_id!));
    expect((await createSpendingUsers(ports)(staff, "finance_approver")).map(person => person.user_id)).toEqual(["eligible"]);
  });
  it("revalidates the saved department head without resolving new participants", async () => {
    const ports = routing(), snapshot = { departmentPolicyVersion: 2 };
    expect(await createSpendingStepCandidates(ports)(staff, "claimant", [], { stage: "department_approval", candidateUserIds: ["head", "new-head"], snapshot }, [])).toEqual(["head"]);
    expect(ports.departmentEligible).toHaveBeenCalledWith("company", "head", snapshot);
    expect(ports.roster).not.toHaveBeenCalled();
    expect(await createSpendingStepCandidates(ports)(staff, "claimant", [], { stage: "department_approval", candidateUserIds: ["head"], snapshot }, ["head"])).toEqual([]);
  });
  it("excludes identity aliases and unsaved participants from an existing accounting pool", async () => {
    const ports = routing();
    vi.mocked(ports.identities).mockImplementation(async (_company, ids) => ids.includes("claimant") ? [...ids, "claimant-alias"] : ids);
    vi.mocked(ports.spendingUsers).mockImplementation(async (_scope, role) => role === "finance_approver" ? [profile("claimant-alias"), profile("saved"), profile("new")] : [profile("payer")]);
    expect(await createSpendingStepCandidates(ports)(staff, "claimant", [], { stage: "accounts_approval", requiredRole: "finance_approver", candidateUserIds: ["claimant-alias", "saved"] }, [])).toEqual(["saved"]);
  });
  it("dispatches saved spending snapshots with prior approved decisions", async () => {
    const ports = routing();
    const instance: ApprovalInstance = { ...staff, id: "instance", record_id: "expense", record_type: "expense", phase: "request", requester_user_id: "claimant", assignee_user_id: null, current_step: 2, cycle: 1, state: "open", blocked_reason: null, version: 4, created_at: 1 };
    const step = { ordinal: 2, stage: "department_approval", route_snapshot_json: JSON.stringify({ spendingPolicy: 1, departmentPolicyVersion: 2 }), approver_pool_json: '["head"]' } as ApprovalStep;
    const previous = { ordinal: 1, state: "approved", approver_user_id: "head" } as ApprovalStep;
    expect(await createExpensePoolCandidates(ports)(instance, step, [previous, step])).toEqual([]);
    expect(ports.departmentEligible).not.toHaveBeenCalled();
  });
});

describe("Expense read projections through injected semantic facts", () => {
  const view = { canDecide: true, blockedReason: null, instance: { state: "in_review" }, currentApprover: { approver_user_id: "head" }, eligibleApproverIds: ["head"], steps: [{ route_snapshot_json: '{"templateVersion":7}' }] };
  it("excludes claimant, accounting preparer and approved participants from Treasury", async () => {
    const roleUsers = vi.fn(async () => ["claimant", "assistant", "accountant", "head", "payer"]);
    const result = await createExpensePayers({ approvedPeople: async () => ["head"], roleUsers })({ ...row, prepared_by: "assistant", accounting_json: '{"preparedBy":"accountant"}' });
    expect(result).toEqual(["payer"]);
    expect(roleUsers).toHaveBeenCalledWith({ company_id: "company", branch_id: "branch", department_id: "department" }, "treasury");
  });
  it("does not query active workflow for a canceled claim", async () => {
    const ports = { approval: vi.fn(async () => view), payers: vi.fn(async () => ["claimant"]), flow: vi.fn(async () => ({ canAct: true })) };
    expect(await createExpenseWorkflowFields(ports)(actor, { ...row, cancelled_at: 123 })).toMatchObject({ status: "cancelled", workflow_stage: "cancelled", can_decide: 0 });
    expect(ports.approval).not.toHaveBeenCalled(); expect(ports.flow).not.toHaveBeenCalled();
  });
  it("keeps flow authority above approval/payment projection permissions", async () => {
    const result = await createExpenseWorkflowFields({ approval: async () => view, payers: async () => ["claimant"], flow: async () => ({ canAct: false, blocker: "Access revoked", current: { primary: { name: "Head" } } }) })(actor, { ...row, workflow_stage: "payment" });
    expect(result).toMatchObject({ can_decide: 0, can_pay: 0, approval_blocked_reason: "Access revoked", current_approver_name: "Head", spending_template_version: 7 });
  });
  it("short-circuits current-reader facts after an authorized participant", async () => {
    const ports = { auditReader: vi.fn(async () => false), participant: vi.fn(async () => true), canDecide: vi.fn(async () => false) };
    expect(await createCurrentExpenseReader(ports)({ ...actor, userId: "prior-reviewer" }, row)).toBe(true);
    expect(ports.canDecide).not.toHaveBeenCalled();
    ports.participant.mockResolvedValue(false);
    expect(await createCurrentExpenseReader(ports)({ ...actor, userId: "outsider" }, row)).toBe(false);
    expect(ports.canDecide).toHaveBeenCalledWith(expect.objectContaining({ userId: "outsider" }), "expense");
  });
  it("returns editable setup blockers without opening approval reads", async () => {
    const approval = vi.fn(async () => view);
    const result = await createExpenseApprovalFields({ requirements: async () => ["Receipt required"], current: async (_actor, record) => record, approval, department: async () => ({ blockedReason: null }) })(actor, { ...row, status: "draft" });
    expect(result).toMatchObject({ setup_requirements_json: '["Receipt required"]', approval_blocked_reason: "Receipt required", can_decide: 0 });
    expect(approval).not.toHaveBeenCalled();
  });
  it("authorizes the claim before loading evidence or related data", async () => {
    const denied = new Error("expense hidden"), related = vi.fn(async () => []);
    const ports = { expense: vi.fn(async () => { throw denied; }), attachments: related, history: related, payments: related, decisions: related, paymentDrafts: related, paymentAttachments: related, approval: vi.fn(async () => view) };
    await expect(createExpenseDetail(ports)(actor, "expense")).rejects.toBe(denied);
    expect(related).not.toHaveBeenCalled(); expect(ports.approval).not.toHaveBeenCalled();
  });
  it("preserves detail shape and pure action projection", async () => {
    const draft = { ...row, status: "draft" }, related = vi.fn(async () => []);
    expect(await createExpenseDetail({ expense: async () => draft, attachments: related, history: related, payments: related, decisions: related, paymentDrafts: related, paymentAttachments: related, approval: async () => view })(actor, "expense")).toEqual({ expense: draft, attachments: [], history: [], payments: [], decisions: [], paymentDrafts: [], paymentAttachments: [], approval: view, permittedActions: ["save", "submit", "cancel"] });
  });
  it("preserves option arrays and actor-selection identifiers", async () => {
    expect(await createExpenseOptions({ people: async () => [{ id: "person" }], accounts: async () => [{ id: "account" }], periods: async () => [{ id: "period" }], payees: async () => [{ id: "payee" }] })({ ...actor, assignmentId: "assignment" })).toEqual({ people: [{ id: "person" }], accounts: [{ id: "account" }], periods: [{ id: "period" }], payees: [{ id: "payee" }], selfUserId: "claimant", companyId: "company", assignmentId: "assignment" });
  });
});

describe("Expense payment and AI evidence source policy", () => {
  const claim = { ...row, accounting_json: "{}", workflow_stage: "draft", company_id: "company", id: "expense" } as AiClaim;
  function eligibility(): ExpenseAiEligibilityPorts {
    return { currentActor: vi.fn(async () => true), canDecide: vi.fn(async () => true), draftPayment: vi.fn(async () => true), approvedPeople: vi.fn(async () => []), treasury: vi.fn(async () => ["payer"]) };
  }
  it("rejects editable evidence when the payment or current independent Treasury changes", async () => {
    const ports = { error: common().error, draft: vi.fn(async () => ({ id: "payment", expense_id: "expense", status: "draft" })), expense: vi.fn(async () => ({ ...row, workflow_stage: "payment" })), payers: vi.fn(async () => ["payer"]) };
    expect(await createExpensePaymentReader(ports)(actor, "payment", false)).toMatchObject({ draft: { id: "payment" } });
    expect(ports.payers).not.toHaveBeenCalled();
    await expect(createExpensePaymentReader(ports)(actor, "payment", true)).rejects.toMatchObject({ code: "TREASURY_REQUIRED" });
    ports.draft.mockResolvedValue({ id: "payment", expense_id: "expense", status: "confirmed" });
    ports.payers.mockClear();
    await expect(createExpensePaymentReader(ports)({ ...actor, userId: "payer" }, "payment", true)).rejects.toMatchObject({ code: "TREASURY_REQUIRED" });
    expect(ports.payers).not.toHaveBeenCalled();
  });
  it("revalidates current AI authority before considering draft ownership", async () => {
    const ports = eligibility(); vi.mocked(ports.currentActor).mockResolvedValue(false);
    expect(await createExpenseAiAllowed(ports)(actor, claim, null)).toBe(false);
    expect(ports.canDecide).not.toHaveBeenCalled();
    ports.currentActor = vi.fn(async () => true);
    expect(await createExpenseAiAllowed(ports)(actor, claim, null)).toBe(true);
    expect(await createExpenseAiAllowed(ports)(actor, claim, "payment")).toBe(false);
  });
  it("uses the existing current reviewer for receipt assistance", async () => {
    const ports = eligibility(), review = { ...claim, workflow_stage: "accounts_review" };
    expect(await createExpenseAiAllowed(ports)(actor, review, null)).toBe(true);
    expect(ports.canDecide).toHaveBeenCalledWith(actor, review);
    vi.mocked(ports.canDecide).mockResolvedValue(false);
    expect(await createExpenseAiAllowed(ports)(actor, review, null)).toBe(false);
  });
  it("requires payment permission and a Treasury identity independent of prior decisions", async () => {
    const ports = eligibility(), paymentClaim = { ...claim, workflow_stage: "payment" }, payer = { ...actor, userId: "payer", role: "treasury" as WorkContext["role"] };
    expect(await createExpenseAiAllowed(ports)(payer, paymentClaim, "payment")).toBe(true);
    vi.mocked(ports.approvedPeople).mockResolvedValue(["payer"]);
    expect(await createExpenseAiAllowed(ports)(payer, paymentClaim, "payment")).toBe(false);
    expect(await createExpenseAiAllowed(ports)(actor, paymentClaim, "payment")).toBe(false);
  });
  it("rejects a removed claim before returning source evidence", async () => {
    await expect(createExpenseAiClaim({ claim: async () => null, error: common().error })("company", "expense")).rejects.toMatchObject({ status: 404, code: "EXPENSE_NOT_FOUND" });
  });
  it("hashes the original manifest with claim and payment versions in the saved order", async () => {
    const raw = '[{"id":"document"}]', digest = vi.fn(async () => "digest");
    const ports: ExpenseAiSourcePorts = { claim: async () => claim, error: common().error, manifest: () => ({ read: async () => raw }), payment: async () => ({ version: 8 }), digest };
    expect(await createExpenseAiSource(ports)("company", "expense", "payment")).toMatchObject({ claim, payment: { version: 8 }, evidence: [{ id: "document" }], digest: "digest", sourceVersion: 8 });
    expect(digest).toHaveBeenCalledWith(JSON.stringify({ raw, version: 4, paymentVersion: 8 }));
  });
  it("captures the manifest before payment lookup and rejects changed payment before evidence I/O", async () => {
    const order: string[] = [], read = vi.fn(async () => "[]"), digest = vi.fn(async () => "digest");
    const ports: ExpenseAiSourcePorts = { claim: async () => { order.push("claim"); return claim; }, error: common().error, manifest: () => { order.push("manifest"); return { read }; }, payment: async () => { order.push("payment"); return null; }, digest };
    await expect(createExpenseAiSource(ports)("company", "expense", "payment")).rejects.toMatchObject({ code: "PAYMENT_CHANGED" });
    expect(order).toEqual(["claim", "manifest", "payment"]);
    expect(read).not.toHaveBeenCalled(); expect(digest).not.toHaveBeenCalled();
  });
  it("denies legacy posting from a scoped actor after refreshing permission", async () => {
    const scoped = { ...actor, branchId: "branch", role: "finance_approver" as WorkContext["role"] }, preparePosting = vi.fn(async () => mutation);
    const ports = { ...common(), expense: async () => ({ ...row, workflow_version: 1 }), legacyPoster: (value: WorkContext) => value, preparePosting };
    await expect(createPostExpense(ports)(scoped, "expense", { expectedVersion: 4, periodId: "period", entryDate: "2026-09-02" }, "legacy-post")).rejects.toMatchObject({ code: "FINANCE_APPROVER_REQUIRED" });
    expect(preparePosting).not.toHaveBeenCalled();
  });
});
