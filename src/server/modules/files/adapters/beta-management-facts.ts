import {betaManagementPeopleSources} from '../../people_payroll/public/beta-management-sources';
import {betaManagementCollaborationSources} from '../../collaboration/public/beta-management-sources';
import {betaManagementTaskSources} from '../../operations/public/beta-management-sources';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
export function betaManagementFacts(db:SqlDatabase){const people=betaManagementPeopleSources(db),collaboration=betaManagementCollaborationSources(db),tasks=betaManagementTaskSources(db);return {async files(companyId:string,companyId2:string,companyId3:string,companyId4:string,companyId5:string,companyId6:string){return db
              .prepare(
                `SELECT COALESCE(SUM(a.bytes),0)+COALESCE((SELECT SUM(bytes) FROM attachment_derivatives WHERE company_id=?),0) bytes FROM attachments a WHERE a.revoked_at IS NULL AND (
              (a.owner_type='social_draft' AND ${collaboration.draftFileOwner}) OR
              (a.owner_type='staff_task' AND ${tasks.taskFileOwner}) OR
              (a.owner_type='expense_claim' AND ${people.expenseFileOwner}) OR
              (a.owner_type='expense_payment' AND ${people.expensePaymentFileOwner}) OR
              (a.owner_type='leave_request' AND ${people.leaveFileOwner}))`,
              )
              .bind(
                companyId,
                companyId2,
                companyId3,
                companyId4,
                companyId5,
                companyId6,
              )
              .first();}};}
