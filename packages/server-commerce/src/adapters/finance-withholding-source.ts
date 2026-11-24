import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {COMMERCE_RETURN_INVOICE_UNREJECTED_GUARD_SQL} from "./guarded-source-dedupe-sql";
export function withholdingCreditGuard(db: SqlDatabase, actor: WorkContext, key: string, invoiceId: string) { return assertion(db, key, `credit-${invoiceId}`, COMMERCE_RETURN_INVOICE_UNREJECTED_GUARD_SQL, [invoiceId]); }
