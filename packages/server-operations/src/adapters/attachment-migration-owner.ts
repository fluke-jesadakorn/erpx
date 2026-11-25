import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {
  OPERATIONS_RD_ATTACHMENT_OWNER_SCOPE,
  nativeOperationsReadFirst,
} from './native-operations-read';

type OwnerScope = {
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  version: number;
};

/** Legacy attachment owner scope for Operations. The retired inline
 *  staff_task descriptor SQL now lives in the native read registry; this
 *  adapter only admits the descriptor and plans the owner read on the caller's
 *  open unit, returning null for unknown kinds exactly as before. */
export async function legacyAttachmentScopeoperations(
  db: SqlDatabase,
  kind: string,
  id: string,
): Promise<OwnerScope | null> {
  if (kind !== 'staff_task') return null;
  return nativeOperationsReadFirst<OwnerScope>(
    OPERATIONS_RD_ATTACHMENT_OWNER_SCOPE,
    [id],
    db,
  );
}
