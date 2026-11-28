import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal governance statements. */
export function legalGovernance(db: SqlDatabase, companyId: string, userId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId, recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    createForm: (f: { key: string; companyId: string; name: string; fieldsJson: string; slaDays: number; actorId: string }) =>
      call('legal_intake_form_create', f.key, [f.key, f.companyId, f.name, f.fieldsJson, f.slaDays, f.actorId]),
    approveForm: (f: { actorId: string; formId: string }) =>
      call('legal_intake_form_approve', '', [f.actorId, f.formId]),
    applyIntake: (f: { formId: string; answersJson: string; now: number; fact3: string; fact4: string | null; itemId: string }) =>
      call('legal_intake_apply', '', [f.formId, f.answersJson, f.now, f.fact3, f.fact4, f.itemId]),
    createAuthority: (f: {
      key: string; itemId: string; companyId: string; userId: string; documentId: string;
      currency: string; limitMinor: number; validFrom: string; validUntil: string; actorId: string;
    }) => call('legal_authority_create', f.key,
      [f.key, f.itemId, f.companyId, f.userId, f.documentId, f.currency, f.limitMinor, f.validFrom, f.validUntil, f.actorId]),
    decideAuthority: (f: { fact0: string; fact1: string | null; authorityId: string }) =>
      call('legal_authority_decide', '', [f.fact0, f.fact1, f.authorityId]),
    advanceRecord: (f: { now: number; itemId: string }) =>
      call('legal_advance_record', '', [f.now, f.itemId]),
    event: (f: { id: string; action: string; note: string; version: number; identifier: string; now: number }) =>
      call('legal_approval_event', f.identifier, [f.identifier, f.id, f.action, userId, f.note, f.version, f.now]),
  };
}
