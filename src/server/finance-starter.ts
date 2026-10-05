import "server-only";
import { z } from "zod";
import type { WorkContext } from "@/server/authorization";
import { requirePermission } from "./iam-permissions";
import { holdingResources } from "./runtime";
import { financeTransaction } from "./finance-transactions";
import { assertion, executeCommand, replayCommand } from "./commands";
import { requireFinanceMoneyScale } from "./finance-money-policy";
import { ApiError } from "./http";
import { thaiFinanceStarterAccounts } from "@/lib/finance-starter";
import { iamRoles } from "./iam-store";
import type { SqlDatabase } from "./runtime/types";

export async function financeSetupAuthority(
  db: SqlDatabase,
  companyId: string,
) {
  const now = Date.now(),
    rows = (
      await db
        .prepare(
          "SELECT DISTINCT user_id,role FROM assignments WHERE company_id=? AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=? AND valid_until>? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?) ORDER BY user_id,role",
        )
        .bind(companyId, now, now, now, now)
        .all<{ user_id: string; role: string }>()
    ).results,
    roles = await iamRoles(
      db,
      companyId,
      rows.map((r) => r.role),
    ),
    preparers = [
      ...new Set(
        rows
          .filter((r) =>
            roles.get(r.role)?.permissions.includes("finance.write"),
          )
          .map((r) => r.user_id),
      ),
    ],
    approvers = [
      ...new Set(
        rows
          .filter((r) =>
            roles.get(r.role)?.permissions.includes("finance.approve"),
          )
          .map((r) => r.user_id),
      ),
    ];
  const independent = Boolean(
    await db
      .prepare(
        "SELECT EXISTS(SELECT 1 FROM jsonb_array_elements_text(?::jsonb) p CROSS JOIN jsonb_array_elements_text(?::jsonb) a WHERE NOT iam_same_person(p.value,a.value,?)) ready",
      )
      .bind(JSON.stringify(preparers), JSON.stringify(approvers), companyId)
      .first("ready"),
  );
  return {
    preparerCount: preparers.length,
    approverCount: approvers.length,
    independent,
  };
}

export const financeStarterSchema = z.object({
  expectedVersion: z.literal(0),
  preset: z.literal("thai_services_v1"),
  name: z.string().trim().min(1).max(160),
  firstMonth: z.string().regex(/^[1-9]\d{3}-(0[1-9]|1[0-2])$/),
  organizationDimensions: z.boolean().default(true),
  reason: z.string().trim().min(3).max(2000),
});
/** Configuration only: policy/openings and every money posting keep their normal independent owner. */
export async function createFinanceStarter(
  actor: WorkContext,
  raw: z.input<typeof financeStarterSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  if (actor.scope.kind !== "holding")
    throw new ApiError(
      403,
      "COMPANY_FINANCE_REQUIRED",
      "Use a Company Finance assignment for initial ledger configuration.",
    );
  const input = financeStarterSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    id = `starter-chart-${actor.companyId}`,
    op = "finance.starter.create";
  try {
    return await financeTransaction(write, async (db) => {
      const replay = await replayCommand(db, actor, key, op, id, input);
      if (replay) return replay;
      await requireFinanceMoneyScale(db, actor.companyId, "THB");
      const company = await db
        .prepare(
          "SELECT id FROM organization_nodes WHERE id=? AND kind='company' AND status='active' FOR UPDATE",
        )
        .bind(actor.companyId)
        .first();
      if (!company)
        throw new ApiError(
          404,
          "COMPANY_NOT_FOUND",
          "Choose an active Company.",
        );
      if (
        await db
          .prepare("SELECT 1 FROM finance_charts WHERE company_id=?")
          .bind(actor.companyId)
          .first()
      )
        throw new ApiError(
          409,
          "STARTER_ALREADY_CONFIGURED",
          "This Company already has a chart. Review its existing configuration or use the account import workflow.",
        );
      const [year, month] = input.firstMonth.split("-").map(Number),
        start = `${input.firstMonth}-01`,
        end = new Date(Date.UTC(year, month, 0)).toISOString().slice(0, 10),
        now = Date.now(),
        periodId = `starter-period-${actor.companyId}-${input.firstMonth}`;
      if (
        await db
          .prepare(
            "SELECT 1 FROM finance_periods WHERE company_id=? AND start_date<=? AND end_date>=?",
          )
          .bind(actor.companyId, end, start)
          .first()
      )
        throw new ApiError(
          409,
          "STARTER_PERIOD_OVERLAP",
          "Review the existing accounting period before creating the starter configuration.",
        );
      const nodes = input.organizationDimensions
        ? (
            await db
              .prepare(
                "SELECT id,kind,display_name FROM organization_nodes WHERE company_id=? AND kind IN ('branch','department') AND status='active' ORDER BY kind,id",
              )
              .bind(actor.companyId)
              .all<{ id: string; kind: string; display_name: string }>()
          ).results
        : [];
      const statements = [
        assertion(
          db,
          key,
          "no-existing-chart",
          "NOT EXISTS(SELECT 1 FROM finance_charts WHERE company_id=?)",
          [actor.companyId],
        ),
        db
          .prepare(
            "INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,created_by,created_at,updated_at) VALUES(?,?,?,'THB','Company policy pending review',?,?,?)",
          )
          .bind(id, actor.companyId, input.name, actor.userId, now, now),
        ...thaiFinanceStarterAccounts.map((a) =>
          db
            .prepare(
              "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',?,?,?,?)",
            )
            .bind(
              `${id}-${a.code}`,
              id,
              actor.companyId,
              a.code,
              a.name,
              a.category,
              a.controlKind === "none" ? 0 : 1,
              a.controlKind,
              now,
              now,
            ),
        ),
        db
          .prepare(
            "INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES(?,?,?,?,?,?)",
          )
          .bind(periodId, actor.companyId, input.firstMonth, start, end, now),
      ];
      for (const kind of ["branch", "department"] as const) {
        const values = nodes.filter((n) => n.kind === kind);
        if (!values.length) continue;
        const dimensionId = `starter-${actor.companyId}-${kind}`;
        statements.push(
          db
            .prepare(
              "INSERT INTO finance_dimension_definitions(id,company_id,code,name,required,created_by,created_at) VALUES(?,?,?,?,0,?,?)",
            )
            .bind(
              dimensionId,
              actor.companyId,
              kind,
              kind === "branch" ? "Branch" : "Department",
              actor.userId,
              now,
            ),
        );
        for (const node of values)
          statements.push(
            db
              .prepare(
                "INSERT INTO finance_dimension_values(id,company_id,dimension_id,code,name,created_at) VALUES(?,?,?,?,?,?)",
              )
              .bind(
                `${dimensionId}-${node.id}`,
                actor.companyId,
                dimensionId,
                node.id,
                node.display_name.slice(0, 160),
                now,
              ),
          );
      }
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
          periodId,
          currency: "THB",
          preset: input.preset,
          version: 1,
          accountCount: thaiFinanceStarterAccounts.length,
          policyReviewRequired: true,
        },
        statements,
      );
    });
  } catch (error) {
    const databaseError = error instanceof ApiError ? error.cause : error;
    if (
      databaseError instanceof Error &&
      "code" in databaseError &&
      databaseError.code === "23505"
    ) {
      const replay = await replayCommand(write, actor, key, op, id, input);
      if (replay) return replay;
      throw new ApiError(
        409,
        "STARTER_CONFIGURATION_CONFLICT",
        "Another configuration already uses these settings. Refresh the Company setup; keep existing dimensions or turn off their starter import.",
      );
    }
    throw error;
  }
}
