import 'server-only';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {COMMERCE_RETURN_INVOICE_UNREJECTED_GUARD_SQL} from "./guarded-source-dedupe-sql";
export function withholdingCreditGuard(db: SqlDatabase, actor: WorkContext, key: string, invoiceId: string) { return assertion(db, key, `credit-${invoiceId}`, COMMERCE_RETURN_INVOICE_UNREJECTED_GUARD_SQL, [invoiceId]); }
