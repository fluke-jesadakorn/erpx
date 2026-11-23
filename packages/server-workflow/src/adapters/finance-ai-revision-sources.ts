import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { nativeWorkflowReadFirst } from './native-workflow-read';
export const financeSourceRevisionSql = `(SELECT count(*) FROM command_receipts WHERE authority_json::jsonb->>'companyId'=? AND (operation LIKE 'finance.%' OR operation LIKE 'commerce.%' OR operation LIKE 'expense.%' OR operation LIKE 'pms.%' OR operation LIKE 'people.payroll.%') AND operation NOT LIKE 'finance.ai.%' AND operation NOT LIKE 'expense.ai.%')`;
export async function financeSourceRevision(db: SqlDatabase, companyId: string) { return Number((await nativeWorkflowReadFirst<{revision: number}>('wf_native_finance_source_revision', [companyId], companyId, db))?.revision ?? 0); }
