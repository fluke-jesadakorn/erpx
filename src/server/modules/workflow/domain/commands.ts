import type {WorkContext} from "../../identity_access/public/authorization";
import {BusinessRuleError} from "../../shared";
export type CommandActor = Pick<WorkContext, "userId" | "assignmentId" | "role" | "companyId" | "branchId" | "departmentId" | "authority" | "iam">;
export const financeFailures:Record<string,string>={
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
