import type {WorkContext} from "../../identity_access/public/authorization";
import {BusinessRuleError} from "../../shared";
export type CommandActor = Pick<WorkContext, "userId" | "assignmentId" | "role" | "companyId" | "branchId" | "departmentId" | "authority" | "iam">;
export const financeFailures:Record<string,string>={
      PAYROLL_REMITTANCE_SOURCE_CHANGED:'The original Payroll liability or exact settlement evidence changed. Review its original owner workflow.',
      PAYROLL_BANK_PROVENANCE_REQUIRED:'This bank observation belongs to its exact original Payroll liability. Settle it from that owner workflow.',
      MANUFACTURING_POSTING_SOURCE_CHANGED:"The original Company posting mapping or its Manufacturing confirmation changed. Review the unchanged owner source before continuing.",
      INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED:"The original initial physical count source changed. Review its original Stock workflow before continuing.",
      SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED:"The original Service initial physical Count source changed. Review its original Stock workflow before continuing.",
      SERVICE_PARTS_POSTING_SOURCE_CHANGED:"The original Service posting source changed. Review the unchanged Company mapping before continuing.",
      POS_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED:"The original POS initial physical Count source changed. Review its original Stock workflow before continuing.",
      MANUFACTURING_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED:"The original initial physical count source changed. Review its original Stock workflow before continuing.",
      REFUND_BANK_PROVENANCE_REQUIRED:"This bank evidence belongs to its original refund instruction. Confirm it from that owner workflow.",
      PERIOD_CLOSED:"The posting date is covered by a closed or closing Company period. Use the next open date or independently reopen it.",
      CLOSE_NOT_READY:"Resolve every draft dated within this Company period before closing, including drafts retained under an overlapping annual period.",
      BOOK_ACCOUNT_MAPPING_REQUIRED:"Complete the effective secondary-book account mapping before posting.",
      BOOK_ACCOUNT_IDENTITY_CHANGED:"The source and secondary accounts must keep the same category, currency and control kind.",
      BOOK_CURRENCY_OR_STATUS_CHANGED:"Review both active books and their functional currency before posting.",
      BOOK_POSTING_PRECEDES_REVIEWED_OPENING:"Use an open date on or after the reviewed secondary-book opening.",
      SECONDARY_BOOK_REQUIRES_OWNED_ADJUSTMENT:"Secondary-book changes require an independently reviewed book adjustment.",
      DIMENSION_VALUE_REQUIRED:"Fill in the Company's required reporting dimensions with active values before posting.",
    };

export function financialConstraintError(error:unknown){const code=Object.keys(financeFailures).find(code=>String(error).includes(code));return code?new BusinessRuleError(409,code,financeFailures[code]):null;}
