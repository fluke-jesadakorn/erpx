import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { AuthorizedSourceRecord } from '@erpx/server-records/public/source-record-contract';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { pmsSourcesSql, pmsSourceReadableSql } from '@erpx/server-pms/public/pms/source-policy';
export function recordPmsSourceReadGuard(database: SqlDatabase, a: WorkContext, key: string, source: AuthorizedSourceRecord, step: string) { return assertion(database, key, step, `EXISTS(SELECT 1 FROM (${pmsSourcesSql}) s WHERE s.id=? AND s.company_id=? AND s.version=? AND ${pmsSourceReadableSql("s", "?::text", "?::text")})`, [source.id, a.companyId, source.version, a.userId, a.assignmentId]); }
