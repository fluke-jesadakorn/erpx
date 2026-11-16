import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function OperationsExceptionsOrganizationFacts(database: SqlDatabase) { return { stageReviewQueuePolicySteps(companyId: string) { return database.prepare("SELECT id FROM organization_nodes WHERE id=? FOR UPDATE").bind(companyId); } }; }
