import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { SqlDatabase, SqlResult } from '@erpx/platform-runtime-server/public/runtime/types';
import type { Row } from '@erpx/workflow-server/public/flow-routing-contract';
import {
  nativeLane14Document,
  nativeLane14Page,
  nativeLane14ReferenceRows,
} from './native-lane-14/client';

/**
 * Commerce workflow order reads. The owner axum service executes the SQL,
 * projects the rows and enforces Company scope; this module only transports
 * the bounded identities. `database` stays in each signature so the Workflow
 * owner's call sites do not change, but no statement is issued on it.
 */
function requireOrder(kind: string): void {
  if (kind !== 'order')
    throw new ApiError(503, 'COMMERCE_LANE14_READ_UNSUPPORTED', 'Use the Commerce order workflow read.');
}

export function workflowDocument(
  _database: SqlDatabase,
  kind: 'order',
  companyId: string,
  id: string,
): Promise<Row | null> {
  requireOrder(kind);
  return nativeLane14Document(companyId, id);
}

export function workflowDocumentPage(
  _database: SqlDatabase,
  kind: 'order',
  companyId: string,
  after: string,
): Promise<SqlResult<Row>> {
  requireOrder(kind);
  return nativeLane14Page(companyId, after);
}

export function workflowReferenceRows(
  _database: SqlDatabase,
  kind: 'order',
  companyId: string,
): Promise<SqlResult<Row>> {
  requireOrder(kind);
  return nativeLane14ReferenceRows(companyId);
}
