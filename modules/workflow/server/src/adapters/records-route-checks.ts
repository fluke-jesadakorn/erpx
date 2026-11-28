import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import type { ApprovalRoute } from '@erpx/operations-server/public/staff-policy-contract';
import {assertion} from "@erpx/workflow-server/public/command-assertions";
export function stageRecordRouteCheck(db: SqlDatabase, key: string, step: string, check: NonNullable<ApprovalRoute['checks']>[number]) { return assertion(db, key, step, check.query, check.args); }
