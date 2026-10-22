import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { ApprovalRoute } from '@erpx/server-operations/public/staff-policy-contract';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function stageRecordRouteCheck(db: SqlDatabase, key: string, step: string, check: NonNullable<ApprovalRoute['checks']>[number]) { return assertion(db, key, step, check.query, check.args); }
