import { projectReadProjection as projectprojectsProjection } from '@erpx/server-projects/public/project-read-projections';
import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function projectactualsSources(db: SqlDatabase) {
    return { async costSources(companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null, branch_id: string | null, branch_id2: string | null, department_id: string | null, department_id2: string | null) {
            return db.prepare(`SELECT l.id source_line_id,j.id journal_id,j.journal_number,j.entry_date,j.currency,
      l.account_id,ac.code account_code,ac.name account_name,l.debit_minor::text debit_minor,
      (l.debit_minor-${projectprojectsProjection.allocatedExpense})::text remaining_minor
    FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
    JOIN finance_accounts ac ON ac.id=l.account_id
    WHERE j.company_id=? AND j.status='posted' AND l.debit_minor>0 AND ac.category IN ('expense','other_expense')
      AND (?::text IS NULL OR l.dimension_json::jsonb->>'branchId'=?)
      AND (?::text IS NULL OR l.dimension_json::jsonb->>'departmentId'=?)
      AND (l.dimension_json::jsonb->>'branchId' IS NULL OR ?::text IS NULL OR l.dimension_json::jsonb->>'branchId'=?)
      AND (l.dimension_json::jsonb->>'departmentId' IS NULL OR ?::text IS NULL OR l.dimension_json::jsonb->>'departmentId'=?)
      AND l.debit_minor>${projectprojectsProjection.allocatedExpense}
    ORDER BY j.entry_date DESC,j.journal_number DESC,l.ordinal LIMIT 250`)
                .bind(companyId, branchId, branchId2, departmentId, departmentId2, branch_id, branch_id2, department_id, department_id2)
                .all<Record<string, string | number>>();
        },
        async expenseLine(sourceLineId: string, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null, branch_id: string | null, branch_id2: string | null, department_id: string | null, department_id2: string | null) {
            return db.prepare(`SELECT l.id,l.debit_minor,j.currency,j.entry_date,j.status journal_status,ac.category
    FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id JOIN finance_accounts ac ON ac.id=l.account_id
    WHERE l.id=? AND j.company_id=? AND (?::text IS NULL OR l.dimension_json::jsonb->>'branchId'=?)
      AND (?::text IS NULL OR l.dimension_json::jsonb->>'departmentId'=?)
      AND (l.dimension_json::jsonb->>'branchId' IS NULL OR ?::text IS NULL OR l.dimension_json::jsonb->>'branchId'=?)
      AND (l.dimension_json::jsonb->>'departmentId' IS NULL OR ?::text IS NULL OR l.dimension_json::jsonb->>'departmentId'=?)`)
                .bind(sourceLineId, companyId, branchId, branchId2, departmentId, departmentId2, branch_id, branch_id2, department_id, department_id2)
                .first<{
                id: string;
                debit_minor: string;
                currency: string;
                entry_date: string;
                journal_status: string;
                category: string;
            }>();
        },
        guardExpenseSource(key: string, value: "posted-expense-source", id: string, companyId: string, id2: string, companyId2: string, branchId: string | null, branchId2: string | null, branch_id: string | null, departmentId: string | null, departmentId2: string | null, department_id: string | null) {
            return assertion(db, key, value, `EXISTS(SELECT 1 FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
      JOIN finance_accounts ac ON ac.id=l.account_id ${projectprojectsProjection.expenseProject}
      WHERE l.id=? AND j.company_id=? AND j.status='posted'
      AND (?::text IS NULL OR l.dimension_json::jsonb->>'branchId'=? AND p.branch_id=?)
      AND (?::text IS NULL OR l.dimension_json::jsonb->>'departmentId'=? AND p.department_id=?)
      AND (l.dimension_json::jsonb->>'branchId' IS NULL OR p.branch_id IS NULL OR l.dimension_json::jsonb->>'branchId'=p.branch_id)
      AND (l.dimension_json::jsonb->>'departmentId' IS NULL OR p.department_id IS NULL OR l.dimension_json::jsonb->>'departmentId'=p.department_id)
      AND l.debit_minor>0 AND ac.category IN ('expense','other_expense'))`, [id, companyId, id2, companyId2,
                branchId, branchId2, branch_id, departmentId, departmentId2, department_id]);
        },
        guardAllocationCap(key: string, value: "project-cost-cap", id: string, amountMinor: number, id2: string) {
            return assertion(db, key, value, `EXISTS(SELECT 1 FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
      WHERE l.id=? AND j.status='posted' AND l.debit_minor>=?+${projectprojectsProjection.otherAllocation})`, [id, amountMinor, id2]);
        } };
}
