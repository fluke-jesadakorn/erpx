import type { WorkContext } from '../../identity_access/public/authorization';
import type { SqlDatabase } from '../../runtime/public/runtime/types';
import type { AuthorizedSourceRecord } from '../../records/public/source-record-contract';
import { assertion } from '../../workflow/public/commands';
import { pmsSourcesSql, pmsSourceReadableSql } from '../public/pms/source-policy';
export function recordPmsSourceReadGuard(database: SqlDatabase, a: WorkContext, key: string, source: AuthorizedSourceRecord, step: string) { return assertion(database, key, step, `EXISTS(SELECT 1 FROM (${pmsSourcesSql}) s WHERE s.id=? AND s.company_id=? AND s.version=? AND ${pmsSourceReadableSql("s", "?::text", "?::text")})`, [source.id, a.companyId, source.version, a.userId, a.assignmentId]); }
