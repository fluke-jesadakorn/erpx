import { inventoryExecutiveOwnerProjections as financeProjectionSources } from "@erpx/finance-server/public/inventory-executive-owner-projections";
import { inventoryExecutiveOwnerProjections as people_payrollProjectionSources } from "@erpx/people-payroll-server/public/inventory-executive-owner-projections";

/** Byte-identical milestone JOIN shared by the paged search, by-id reader and
 * grouped executive metric read. The caller owns the FROM alias and indentation;
 * only the source identity varies. */
export const projectMilestoneProjectJoin = (sourceSql: string, indent = '') =>
    `${indent}JOIN (${sourceSql}) p ON p.id=m.project_id AND p.company_id=m.company_id`;
/** Byte-identical owner projection join shared by the milestone evidence reads. */
export const projectMilestoneOwnerJoin = (indent = '') =>
    `${indent}LEFT JOIN (${people_payrollProjectionSources.hr_people}) h ON h.company_id=m.company_id AND h.user_id=m.owner_user_id AND h.merged_into_id IS NULL`;
/** Byte-identical posted-cost FROM/JOIN chain shared by the paged search, by-id
 * reader and grouped executive metric read; the caller owns the first indent. */
export const projectPostedCostFrom = (sourceSql: string) => `FROM staff_project_cost_allocations a JOIN (${sourceSql}) p ON p.id=a.project_id AND p.company_id=a.company_id
      JOIN (${financeProjectionSources.finance_journal_lines}) l ON l.id=a.journal_line_id JOIN (${financeProjectionSources.finance_journals}) j ON j.id=l.journal_id
      JOIN (${financeProjectionSources.finance_accounts}) ac ON ac.id=l.account_id`;
/** Byte-identical work-log FROM/JOIN chain shared by the work-log evidence
 * source and the grouped executive metric read; the caller owns the indent. */
export const projectActualHoursFrom = (sourceSql: string) =>
    `FROM staff_project_work_logs l JOIN (${sourceSql}) p ON p.id=l.project_id AND p.company_id=l.company_id`;
