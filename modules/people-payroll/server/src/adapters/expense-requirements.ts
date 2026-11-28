

import { hospitalityOwnerProjectionSources as operationsProjectionSources } from "@erpx/operations-server/public/hospitality-owner-projection-sources";
import { ppRd55AttachmentPresenceV2, ppRd55AttachmentPresenceV1 } from './pp-read-tail-r55-sources';
import { createExpenseRequirements, type ExpenseRequirementsPorts } from "@erpx/people-payroll-server/application/expense-evidence";

import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import "server-only";

import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { activeStaff } from "@erpx/operations-server/public/staff-policy";

import {type ExpenseRow} from '@erpx/people-payroll-server/adapters/expenses';
import { attachmentsV2Readable } from "@erpx/files-server/public/attachments/feature";

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
        ? ppRd55AttachmentPresenceV2('probe', false, false, false)
        : ppRd55AttachmentPresenceV1('probe', false, false, false)).bind(row.id).first());
    },
  });
}
export async function expenseRequirements(db: SqlDatabase, row: ExpenseRow) {
  return nativeExpenseRequirements(db)(row);
}
