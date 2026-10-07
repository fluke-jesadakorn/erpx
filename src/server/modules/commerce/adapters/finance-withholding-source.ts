import 'server-only';
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import { assertion } from '@/server/modules/workflow/public/commands';
export function withholdingCreditGuard(db: SqlDatabase, actor: WorkContext, key: string, invoiceId: string) { return assertion(db, key, `credit-${invoiceId}`, "NOT EXISTS(SELECT 1 FROM commerce_returns WHERE invoice_id=? AND status!='rejected')", [invoiceId]); }
