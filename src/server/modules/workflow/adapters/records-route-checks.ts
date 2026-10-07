import type { SqlDatabase } from '../../runtime/public/runtime/types';
import type { ApprovalRoute } from '../../operations/public/staff-policy-contract';
import { assertion } from '../public/commands';
export function stageRecordRouteCheck(db: SqlDatabase, key: string, step: string, check: NonNullable<ApprovalRoute['checks']>[number]) { return assertion(db, key, step, check.query, check.args); }
