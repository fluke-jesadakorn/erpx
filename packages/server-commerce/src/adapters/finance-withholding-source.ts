import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function withholdingCreditGuard(db: SqlDatabase, actor: WorkContext, key: string, invoiceId: string) { return assertion(db, key, `credit-${invoiceId}`, "NOT EXISTS(SELECT 1 FROM commerce_returns WHERE invoice_id=? AND status!='rejected')", [invoiceId]); }
