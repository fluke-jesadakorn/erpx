import 'server-only';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { fieldCollaborationCommand } from './native-lane-15/field-command-client';

/**
 * Immutable Collaboration routing facts for Field. Both `collaboration_*`
 * SELECTs and the boolean projection are now owned end to end by the native
 * lane-15 axum router; this module is a strict transport with no SQL and no
 * TypeScript fallback. The chosen owner still authorizes the complete
 * command/read — no source body, grant, receipt or write is returned here.
 */
export function nativeFieldCollaborationCommand(actor: WorkContext, input: unknown): Promise<boolean> {
  return fieldCollaborationCommand(actor, input);
}
