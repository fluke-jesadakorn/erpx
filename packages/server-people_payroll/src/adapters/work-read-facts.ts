import {workforcePurchasedHistorySql} from "@erpx/server-people_payroll/adapters/workforce-purchases";
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkRow} from "@erpx/feature-work_projection/lib/work-queue";
import {participantSql} from '@erpx/server-workflow/public/approval-participant-projection';
export function workReadPeoplePayrollFacts(db:SqlDatabase){return {pendingLeave:async(facts:{companyId:string;})=>db.prepare(`SELECT id FROM leave_requests WHERE company_id=? AND state='pending_approval' AND ${await workforcePurchasedHistorySql("leave_requests.company_id")} LIMIT 250`).bind(facts.companyId).all<{ id: string }>(),
visibleLeave:async(facts:{companyId:string;userId:string;assignmentId:string;})=>db.prepare(`SELECT l.id,l.number,l.state,l.start_date,l.days,l.created_by,l.updated_at,l.handover_note,l.cancellation_state FROM leave_requests l WHERE l.company_id=? AND ${await workforcePurchasedHistorySql("l.company_id")} AND (l.created_by=? OR l.decided_by=? OR ${participantSql("leave", "l", "?", "?")} OR ${participantSql("leave_cancellation", "l", "?", "?")}) ORDER BY l.updated_at DESC,l.id LIMIT 250`).bind(facts.companyId,facts.userId,facts.userId,facts.userId,facts.assignmentId,facts.userId,facts.assignmentId).all<WorkRow>()};}
