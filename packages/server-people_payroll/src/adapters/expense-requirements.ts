

import { hospitalityOwnerProjectionSources as filesProjectionSources } from "@erpx/server-files/public/hospitality-owner-projection-sources";

import { hospitalityOwnerProjectionSources as operationsProjectionSources } from "@erpx/server-operations/public/hospitality-owner-projection-sources";
import { createExpenseRequirements, type ExpenseRequirementsPorts } from "@erpx/server-people_payroll/application/expense-evidence";

import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";

import { ApiError } from "@erpx/server-runtime/public/http";
import { activeStaff } from "@erpx/server-operations/public/staff-policy";

import {type ExpenseRow} from '@erpx/server-people_payroll/adapters/expenses';
import { attachmentsV2Readable } from "@erpx/server-files/public/attachments/feature";

export function nativeExpenseEmployment(db: SqlDatabase): ExpenseRequirementsPorts["employment"] {
  return async row => {
    try {
      const staff = await activeStaff(db, String(row.company_id), String(row.claimant_user_id));
      const level = staff.level_id && await db.prepare(`SELECT 1 FROM (${operationsProjectionSources.staff_levels}) staff_levels WHERE id=? AND company_id=? AND active=1`).bind(staff.level_id, row.company_id).first();
      return { startDate: staff.start_date, endDate: staff.end_date, activeLevel: Boolean(level) };
    } catch (error) {
      if (!(error instanceof ApiError)) throw error;
      return { unavailable: error.message };
    }
  };
}
export function nativeExpenseRequirements(db: SqlDatabase, readReceipt?: (row:ExpenseRow)=>Promise<boolean>, readEmployment: ExpenseRequirementsPorts["employment"] = nativeExpenseEmployment(db)) {
  return createExpenseRequirements({
    employment: readEmployment,
    async hasReceipt(row) {
      if (readReceipt) return readReceipt(row);
      return Boolean(await db.prepare(attachmentsV2Readable()
        ? `SELECT 1 FROM (${filesProjectionSources.attachments}) a JOIN (${filesProjectionSources.attachment_links}) l ON l.attachment_id=a.id
             JOIN (${filesProjectionSources.attachment_sets}) s ON s.id=l.set_id
             WHERE a.owner_type='expense_claim' AND a.owner_id=? AND a.revoked_at IS NULL
               AND a.source_state='available' AND l.state='bound'
               AND s.target_type='expense_claim' AND s.target_id=a.owner_id LIMIT 1`
        : `SELECT 1 FROM (${filesProjectionSources.attachments}) attachments WHERE owner_type='expense_claim' AND owner_id=? AND revoked_at IS NULL LIMIT 1`).bind(row.id).first());
    },
  });
}
export async function expenseRequirements(db: SqlDatabase, row: ExpenseRow) {
  return nativeExpenseRequirements(db)(row);
}
