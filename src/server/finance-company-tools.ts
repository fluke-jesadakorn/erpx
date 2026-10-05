import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { executeCommand, replayCommand, assertion } from "./commands";
import { ApiError } from "./http";
export const financeToolSchema = z.object({
  tool: z.enum([
    "planning",
    "bank_automation",
    "collections",
    "business_contracts",
    "instruments",
    "asset_policies",
    "group_reporting",
  ]),
  enabled: z.boolean(),
  expectedVersion: z.number().int().nonnegative(),
  reason: z.string().trim().min(3).max(2000),
});
export type FinanceTool = z.infer<typeof financeToolSchema>["tool"];
export async function financeCompanyTools(actor: WorkContext) {
  actor = requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  return (
    await write
      .prepare(
        "SELECT * FROM finance_company_tools WHERE company_id=? ORDER BY tool",
      )
      .bind(actor.companyId)
      .all<{ tool: FinanceTool; enabled: number; version: number }>()
  ).results;
}
export async function requireFinanceTool(
  db: SqlDatabase,
  companyId: string,
  tool: FinanceTool,
) {
  if (
    !(await db
      .prepare(
        "SELECT 1 FROM finance_company_tools WHERE company_id=? AND tool=? AND enabled=1",
      )
      .bind(companyId, tool)
      .first())
  )
    throw new ApiError(
      409,
      "FINANCE_TOOL_DISABLED",
      "Enable this Company tool in Finance setup before preparing new work.",
    );
}
export async function setFinanceCompanyTool(
  actor: WorkContext,
  input: z.infer<typeof financeToolSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  input = financeToolSchema.parse(input);
  const { write } = await holdingResources(actor.holdingId),
    id = `${actor.companyId}:${input.tool}`,
    op = "finance.tool.set",
    prior = await replayCommand(write, actor, key, op, id, input);
  if (prior) return prior;
  return executeCommand(
    write,
    actor,
    key,
    op,
    input,
    id,
    input.expectedVersion + 1,
    { id, version: input.expectedVersion + 1, enabled: input.enabled },
    [
      assertion(
        write,
        key,
        "tool-version",
        "COALESCE((SELECT version FROM finance_company_tools WHERE company_id=? AND tool=?),0)=?",
        [actor.companyId, input.tool, input.expectedVersion],
      ),
      write
        .prepare(
          "INSERT INTO finance_company_tools(company_id,tool,enabled,version,changed_by,reason,updated_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(company_id,tool) DO UPDATE SET enabled=excluded.enabled,version=excluded.version,changed_by=excluded.changed_by,reason=excluded.reason,updated_at=excluded.updated_at",
        )
        .bind(
          actor.companyId,
          input.tool,
          input.enabled ? 1 : 0,
          input.expectedVersion + 1,
          actor.userId,
          input.reason,
          Date.now(),
        ),
    ],
  );
}
