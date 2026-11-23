import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { nativeWorkflowReadAll } from "./native-workflow-read";

/** Owner-native read: the statement is planned by the mounted Workflow wave3
 * command planner (WF_NATIVE_SPENDING_SETTINGS_HISTORY) and executed on the
 * caller's open unit. This module carries no SQL for this read. */
export function spendingSettingsHistory(db: SqlDatabase, companyId: string) {
  return nativeWorkflowReadAll<{
    title: string;
    version: number;
    actor: string;
    at: number;
    kind: string;
  }>("wf_native_spending_settings_history", [companyId, companyId], companyId, db);
}
