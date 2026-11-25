import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {nodeExistsGuard} from './organization-read-tail-sources-r50';
export function peopleHirePlacementFacts(database: SqlDatabase) { return { guard13(key: string, value: "hire-placement", branch_id: string, companyId: string, department_id: string | null, department_id2: string | null, companyId2: string, branch_id2: string) { return assertion(database, key, value, `${nodeExistsGuard("status='active'")} AND (?::text IS NULL OR ${nodeExistsGuard("branch_id=? AND status='active'")})`, [branch_id, companyId, department_id, department_id2, companyId2, branch_id2]); } }; }
