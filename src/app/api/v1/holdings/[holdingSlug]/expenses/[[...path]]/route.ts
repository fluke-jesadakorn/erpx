import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { z } from "zod";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import {
  expenseAccountingSchema,
  saveExpenseAccounting,
} from "@/server/modules/people_payroll/expense";
import {
  createExpensePayment,
  confirmExpensePayment,
  saveExpensePayment,
  revokePaymentSlip,
  expensePaymentSchema,
} from "@/server/modules/people_payroll/expense";
import { readExpenseAiStatus, requestExpenseAi } from "@/server/modules/people_payroll/expense";
import {
  reverseExpense,
  expenseReversalSchema,
  listExpenses,
  expenseOptions,
  expenseDetail,
  saveExpense,
  transitionExpense,
  postExpense,
  revokeExpenseAttachment,
  expenseDraftSchema,
  expenseUpdateSchema,
  expenseVersionSchema,
  expenseDecisionSchema,
  expensePostSchema,
} from "@/server/modules/people_payroll/expense";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
export async function GET(_request: Request, context: Context) {
  try {
    const { holdingSlug, path: p = [] } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    if (p.length === 2 && p[1] === "ai") {
      return Response.json(
        {
          data: await readExpenseAiStatus(actor, p[0], new URL(_request.url).searchParams.get("payment")),
        },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    }
    const data =
      p.length === 0
        ? {
            expenses: await listExpenses(actor),
            options: await expenseOptions(actor),
          }
        : p.length === 1
          ? await expenseDetail(actor, p[0])
          : null;
    if (!data) throw new ApiError(404, "NOT_FOUND", "Expense route not found.");
    return Response.json(
      { data },
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
export async function POST(request: Request, context: Context) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
      throw new ApiError(
        403,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
      );
    const { holdingSlug, path: p = [] } = await context.params;
    const actor = await requireWorkContext(holdingSlug),
      key = request.headers.get("Idempotency-Key");
    if (!key)
      throw new ApiError(
        422,
        "COMMAND_KEY_REQUIRED",
        "Idempotency-Key is required.",
      );
    const input = await request.json();
    let data;
    if (p.length === 0)
      data = await saveExpense(
        actor,
        null,
        { ...expenseDraftSchema.parse(input), expectedVersion: 1 },
        key,
      );
    else if (p.length === 2 && p[1] === "ai") {
      data = await requestExpenseAi(
        actor,
        p[0],
        z
          .object({
            expectedVersion: z.number().int().positive(),
            paymentId: z.string().nullable().default(null),
            retry: z.boolean().default(false),
            contentAccessConsent: z.literal(true),
          })
          .parse(input),
        key,
      );
      // The native jobs process polls the committed outbox.
    } else if (p.length === 1)
      data = await saveExpense(
        actor,
        p[0],
        expenseUpdateSchema.parse(input),
        key,
      );
    else if (p.length === 2 && p[1] === "submit")
      data = await transitionExpense(
        actor,
        p[0],
        "submit",
        expenseVersionSchema.parse(input),
        key,
      );
    else if (p.length === 2 && p[1] === "accounting")
      data = await saveExpenseAccounting(
        actor,
        p[0],
        expenseAccountingSchema.parse(input),
        key,
      );
    else if (p.length === 2 && p[1] === "payments")
      data = await createExpensePayment(
        actor,
        p[0],
        expenseVersionSchema.parse(input).expectedVersion,
        key,
      );
    else if (p.length === 4 && p[1] === "payments" && p[3] === "confirm")
      data = await confirmExpensePayment(
        actor,
        p[2],
        expensePaymentSchema.parse(input),
        key,
        p[0],
      );
    else if (p.length === 3 && p[1] === "payments")
      data = await saveExpensePayment(
        actor,
        p[2],
        expensePaymentSchema.parse(input),
        key,
        p[0],
      );
    else if (
      p.length === 6 &&
      p[1] === "payments" &&
      p[3] === "attachments" &&
      p[5] === "revoke"
    )
      data = await revokePaymentSlip(
        actor,
        p[0],
        p[2],
        p[4],
        expenseVersionSchema.parse(input).expectedVersion,
        key,
      );
    else if (p.length === 2 && ["approve", "reject", "cancel"].includes(p[1]))
      data = await transitionExpense(
        actor,
        p[0],
        p[1] as "approve" | "reject" | "cancel",
        expenseDecisionSchema.parse(input),
        key,
      );
    else if (p.length === 2 && p[1] === "reverse")
      data = await reverseExpense(
        actor,
        p[0],
        expenseReversalSchema.parse(input),
        key,
      );
    else if (p.length === 2 && p[1] === "post")
      data = await postExpense(
        actor,
        p[0],
        expensePostSchema.parse(input),
        key,
      );
    else if (p.length === 4 && p[1] === "attachments" && p[3] === "revoke")
      data = await revokeExpenseAttachment(
        actor,
        p[0],
        p[2],
        expenseVersionSchema.parse(input).expectedVersion,
        key,
      );
    else throw new ApiError(404, "NOT_FOUND", "Expense action not found.");
    return Response.json({ data }, { status: p.length === 0 ? 201 : 200 });
  } catch (e) {
    if (e instanceof z.ZodError)
      return errorResponse(
        new ApiError(
          422,
          "VALIDATION_FAILED",
          e.issues[0]?.message ?? "Invalid input.",
        ),
      );
    if (e instanceof SyntaxError)
      return errorResponse(
        new ApiError(422, "INVALID_JSON", "Use valid JSON."),
      );
    return errorResponse(e);
  }
}
