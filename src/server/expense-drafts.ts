import type { SqlDatabase } from "./runtime/types";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { activeStaff } from "./staff-policy";
import { expenseAuthoritySnapshot } from "./expense-routing";
import { canManagePeople } from "./people-directory";
import { decimalToMinor } from "./finance";
import { getExpense, canEditExpense, type ExpenseRow } from "./expenses";
import { attachmentsV2Readable } from "./attachments/feature";

const optionalText = (max: number) => z.string().trim().max(max).optional();
const optionalMoney = z
  .union([z.literal(""), z.string().regex(/^\d+(\.\d{1,2})?$/)])
  .optional();
export const expenseDraftSchema = z.object({
  payeePartyId: optionalText(160),
  requestKind: z.enum(["expense","payment"]).optional(),
  personId: optionalText(160),
  expenseDate: z.union([z.literal(""), z.iso.date()]).optional(),
  merchant: optionalText(200),
  receiptReference: optionalText(100),
  businessPurpose: optionalText(2000),
  currency: z.union([z.literal(""), z.string().regex(/^[A-Z]{3}$/)]).optional(),
  amount: optionalMoney,
  tax: optionalMoney,
  categoryAccountId: optionalText(160),
});
export const expenseUpdateSchema = expenseDraftSchema.extend({
  expectedVersion: z.number().int().positive(),
});

export async function expenseRequirements(db: SqlDatabase, row: ExpenseRow) {
  const missing: string[] = [];
  try {
    const staff = await activeStaff(
      db,
      String(row.company_id),
      String(row.claimant_user_id),
    );
    const level =
      staff.level_id &&
      (await db
        .prepare(
          "SELECT 1 FROM staff_levels WHERE id=? AND company_id=? AND active=1",
        )
        .bind(staff.level_id, row.company_id)
        .first());
    if (!level)
      missing.push(
        "Ask an administrator to set your active staff level in People.",
      );
    if (
      row.expense_date &&
      (String(row.expense_date) < staff.start_date ||
        (staff.end_date && String(row.expense_date) > staff.end_date))
    )
      missing.push("The expense date must fall within your engagement dates.");
  } catch (error) {
    if (!(error instanceof ApiError)) throw error;
    missing.push(error.message);
  }
  if (!row.expense_date) missing.push("Add the expense date.");
  if (String(row.merchant ?? "").length < 2) missing.push("Add the merchant.");
  if (String(row.receipt_reference ?? "").length < 2)
    missing.push("Add the receipt number.");
  if (String(row.business_purpose ?? "").length < 5)
    missing.push("Explain the business purpose.");
  if (!row.amount_minor || !row.currency)
    missing.push("Add a positive amount and currency.");
  if (!row.category_account_id) missing.push("Choose an expense category.");
  if (
    !(await db
      .prepare(
        attachmentsV2Readable()
          ? `SELECT 1 FROM attachments a JOIN attachment_links l ON l.attachment_id=a.id
             JOIN attachment_sets s ON s.id=l.set_id
             WHERE a.owner_type='expense_claim' AND a.owner_id=? AND a.revoked_at IS NULL
               AND a.source_state='available' AND l.state='bound'
               AND s.target_type='expense_claim' AND s.target_id=a.owner_id LIMIT 1`
          : "SELECT 1 FROM attachments WHERE owner_type='expense_claim' AND owner_id=? AND revoked_at IS NULL LIMIT 1",
      )
      .bind(row.id)
      .first())
  )
    missing.push(row.request_kind === "payment" ? "Attach the payment request’s supporting document before submitting." : "Attach the receipt before submitting.");
  return missing;
}

export async function saveExpenseDraft(
  actor: WorkContext,
  id: string | null,
  input: z.infer<typeof expenseUpdateSchema>,
  key: string,
) {
  const { write } = await holdingResources(actor.holdingId),
    recordId = id ?? `expense-${key}`,
    operation = id ? "expense.update" : "expense.create";
  const prior = await replayCommand(
    write,
    actor,
    key,
    operation,
    recordId,
    input,
  );
  if (prior) return prior;
  const current = id ? await getExpense(actor, id) : null;
  const authority = await expenseAuthoritySnapshot(write, actor.companyId);
  if (current && !canEditExpense(actor, current))
    throw new ApiError(
      403,
      "EXPENSE_LOCKED",
      "Return the claim for correction before changing its details.",
    );
  let claimant = String(current?.claimant_user_id ?? actor.userId),
    claimantName = String(current?.claimant_name ?? actor.name),
    personId = current?.employee_id ?? null;
  if (input.personId) {
    const person = await write
      .prepare(
        "SELECT id,user_id,legal_name FROM hr_people WHERE id=? AND company_id=? AND merged_into_id IS NULL",
      )
      .bind(input.personId, actor.companyId)
      .first<{ id: string; user_id: string | null; legal_name: string }>();
    if (!person?.user_id)
      throw new ApiError(
        422,
        "CLAIMANT_UNAVAILABLE",
        "Choose a person with a linked company account.",
      );
    if (current && person.user_id !== claimant)
      throw new ApiError(
        422,
        "CLAIMANT_IMMUTABLE",
        "Create a separate claim for another person.",
      );
    if (person.user_id !== actor.userId && !canManagePeople(actor))
      throw new ApiError(
        403,
        "EXPENSE_OWNER_REQUIRED",
        "Create a claim for yourself.",
      );
    claimant = person.user_id;
    claimantName = person.legal_name;
    personId = person.id;
  }
  const staff = await activeStaff(write, actor.companyId, claimant).catch(
    (error) => {
      if (error instanceof ApiError) return null;
      throw error;
    },
  );
  if (claimant !== actor.userId && (actor.branchId || actor.departmentId)) {
    const personScope =
      staff ??
      (await write
        .prepare(
          "SELECT branch_id,department_id FROM hr_engagements WHERE person_id=? AND company_id=? ORDER BY start_date DESC LIMIT 1",
        )
        .bind(personId, actor.companyId)
        .first<{ branch_id: string | null; department_id: string | null }>());
    if (
      !personScope ||
      (actor.branchId && personScope.branch_id !== actor.branchId) ||
      (actor.departmentId && personScope.department_id !== actor.departmentId)
    )
      throw new ApiError(
        404,
        "PERSON_NOT_FOUND",
        "Person not found in this workspace.",
      );
  }
  if (staff) {
    if (
      claimant !== actor.userId &&
      ((actor.branchId && staff.branch_id !== actor.branchId) ||
        (actor.departmentId && staff.department_id !== actor.departmentId))
    )
      throw new ApiError(
        404,
        "PERSON_NOT_FOUND",
        "Person not found in this workspace.",
      );
    personId = staff.person_id;
    claimantName = staff.legal_name;
  }
  const amount = input.amount ? decimalToMinor(input.amount) : null,
    tax = input.tax ? decimalToMinor(input.tax) : 0;
  if (amount !== null && (amount <= 0 || !Number.isSafeInteger(amount + tax)))
    throw new ApiError(
      422,
      "EXPENSE_AMOUNT_INVALID",
      "Use a positive amount within the supported precision.",
    );
  const category = input.categoryAccountId || null,
    currency = input.currency || null,
    now = Date.now(),
    version = current ? input.expectedVersion + 1 : 1;
  const steps = [
    authority.guard(key),
    assertion(
      write,
      key,
      "claimant-access",
      "EXISTS(SELECT 1 FROM assignments WHERE user_id=? AND company_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>?)",
      [claimant, actor.companyId, now, now],
    ),
  ];
  const requestKind=input.requestKind??current?.request_kind??"expense",payeePartyId=requestKind==="payment"?(input.payeePartyId??current?.payee_party_id??null):null;
  if(payeePartyId)steps.push(assertion(write,key,"supplier-payee","EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind='supplier' AND status='active' AND currency=?)",[payeePartyId,actor.companyId,currency]));
  if (category)
    steps.push(
      assertion(
        write,
        key,
        "category",
        "EXISTS(SELECT 1 FROM finance_accounts a JOIN commerce_settings s ON s.chart_id=a.chart_id AND s.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.category='expense' AND a.status='active' AND (?::text IS NULL OR (a.currency=? AND s.currency=?)))",
        [category, actor.companyId, currency, currency, currency],
      ),
    );
  if (current)
    steps.push(
      assertion(
        write,
        key,
        "expense-version",
        "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND cancelled_at IS NULL AND status IN ('draft','rejected'))",
        [recordId, actor.companyId, input.expectedVersion],
      ),
    );
  const values = [
    requestKind,
    payeePartyId || null,
    personId,
    staff?.engagement_id ?? null,
    staff?.branch_id ?? null,
    staff?.department_id ?? null,
    claimant,
    claimantName,
    input.expenseDate || null,
    input.merchant || null,
    currency,
    amount,
    tax,
    category,
    input.receiptReference || null,
    input.businessPurpose ?? "",
    current?.prepared_by ?? actor.userId,
    now,
  ];
  steps.push(
    current
      ? write
          .prepare(
            "UPDATE finance_expenses SET request_kind=?,payee_party_id=?,employee_id=?,engagement_id=?,branch_id=?,department_id=?,claimant_user_id=?,claimant_name=?,expense_date=?,merchant=?,currency=?,amount_minor=?,tax_minor=?,category_account_id=?,receipt_reference=?,business_purpose=?,prepared_by=?,updated_at=?,status='draft',workflow_version=2,workflow_stage='draft',version=version+1 WHERE id=?",
          )
          .bind(...values, recordId)
      : write
          .prepare(
            "INSERT INTO finance_expenses(request_kind,payee_party_id,employee_id,engagement_id,branch_id,department_id,claimant_user_id,claimant_name,expense_date,merchant,currency,amount_minor,tax_minor,category_account_id,receipt_reference,business_purpose,prepared_by,updated_at,id,company_id,created_by,created_at,workflow_version) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,2)",
          )
          .bind(...values, recordId, actor.companyId, actor.userId, now),
  );
  return executeCommand(
    write,
    actor,
    key,
    operation,
    input,
    recordId,
    version,
    { id: recordId, status: "draft", version },
    steps,
    {
      companyId: actor.companyId,
      branchId: staff?.branch_id ?? null,
      departmentId: staff?.department_id ?? null,
    },
  );
}
