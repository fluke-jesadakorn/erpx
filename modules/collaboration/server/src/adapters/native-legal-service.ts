import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal service statements. */
export function legalService(db: SqlDatabase, companyId: string, userId: string, assignmentsSql: string, subtypeTables: Record<string, string>) {
  const call = (kind: string, key: string, values: unknown[], extra: Record<string, unknown> = {}): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId, recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values), ...extra }, key, 0);
  return {
    accessEvent: (f: { identifier: string; id: string; now: number }) =>
      call('legal_access_event', f.identifier, [f.identifier, f.id, null, userId, 'record.read', f.now]),
    configure: (f: { companyId: string; enabled: boolean; aiEnabled: boolean; reminderDaysJson: string | null; reminderDaysJson2: string | null }) =>
      call('legal_settings_configure', '', [f.companyId, f.enabled, f.aiEnabled, f.reminderDaysJson, f.reminderDaysJson2]),
    createRecord: (f: {
      id: string; companyId: string; branchId: string | null; departmentId: string | null;
      kind: string; title: string; description: string; classification: string; country: string;
      jurisdiction: string; language: string; currency: string; timezone: string; priority: string;
      dueDate: string | null; ownerId: string; actorId: string; now: number; now2: number;
    }) => call('legal_item_create', f.id,
      [f.id, f.companyId, f.branchId, f.departmentId, f.kind, f.title, f.description, f.classification, f.country, f.jurisdiction, f.language, f.currency, f.timezone, f.priority, f.dueDate, f.ownerId, f.actorId, f.now, f.now2]),
    createSubtype: (f: { id: string; dataJson: string; recordKind: string }) =>
      call('legal_subtype_create', '', [f.id, f.dataJson], { table_name: subtypeTables[f.recordKind] }),
    addInitialMember: (f: { id: string; u: string }) =>
      call('legal_member_add', '', [f.id, f.u]),
    updateRecord: (f: {
      title: string; description: string; classification: string; country: string;
      jurisdiction: string; language: string; currency: string; timezone: string;
      priority: string; dueDate: string | null; ownerId: string; id: string;
    }) => call('legal_item_update', '',
      [f.title, f.description, f.classification, f.country, f.jurisdiction, f.language, f.currency, f.timezone, f.priority, f.dueDate, f.ownerId, f.id]),
    updateSubtype: (f: { dataJson: string; id: string; recordKind: string }) =>
      call('legal_subtype_update', '', [f.dataJson, f.id], { table_name: subtypeTables[f.recordKind] }),
    routeRequester: (f: { id: string; companyId: string; branchId: string | null; departmentId: string | null }) =>
      call('legal_requester_route', '', [f.id, f.companyId, f.branchId, f.departmentId], { source_access_sql: assignmentsSql }),
    retainOwnerMembership: (f: { id: string; ownerId: string }) =>
      call('legal_member_retain', '', [f.id, f.ownerId]),
    advanceRecord: (f: { state: string; timestamp: number; id: string }) =>
      call('legal_item_state_advance', '', [f.state, f.timestamp, f.id]),
    expandHoldTargets: () => call('legal_hold_targets_expand', '', []),
    event: (f: { id: string; action: string; note: string; version: number; identifier: string; now: number }) =>
      call('legal_approval_event', f.identifier, [f.identifier, f.id, f.action, userId, f.note, f.version, f.now]),
  };
}
