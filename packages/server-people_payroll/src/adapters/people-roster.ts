import {ppRd36TwoPredTail} from './pp-read-tail-r36-sources';
import {ppRd36SinglePred, ppRd36ThreePred} from './pp-read-tail-r36-sources';
import { ppRd58RosterShiftsByWeekSql } from './pp-read-tail-r58-sources';
import { rosterAssignmentById, rosterAssignmentsByShift } from './pp-read-tail-r53-native';
import { ppRd25ByIdCompany } from './pp-read-tail-r25-sources';
import { nativePpStatement } from '@erpx/server-people_payroll/adapters/native-people-payroll-command';
import "server-only";
import { rosterMonday, type RosterWeek, type RosterShift, type RosterAssignment, type RosterRequest, type ShiftTemplate, type RosterHistory } from "@erpx/feature-people_payroll/lib/people-roster";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { activeStaff, rosterStaffPeople } from "@erpx/server-operations/public/staff-policy";
import { ApiError } from "@erpx/server-runtime/public/http";
import { executeCommand, replayCommand } from "@erpx/server-workflow/public/commands";
import { isTransactionalSqlDatabase, type SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { type ShiftWithWeek } from '@erpx/server-people_payroll/domain/people-roster';
import { createPeopleRosterApplication, type PeopleRosterPorts } from '@erpx/server-people_payroll/application/people-roster';
import { rosterOrganizationFacts } from '@erpx/server-organization/public/people-roster-facts';
import { rosterNotifications } from '@erpx/server-collaboration/adapters/native-roster-notifications';
import { rosterIdentityFacts } from '@erpx/server-identity_access/public/people-roster-facts';
import { rosterCommandSchema, type RosterCommand } from '@erpx/server-people_payroll/domain/roster';
import { rosterPurchasedAccess, rosterPurchasePurpose, withRosterCoveragePurchasedTransaction, withRosterPurchasedTransaction } from '@erpx/server-people_payroll/adapters/roster-purchases';
const pending = "('pending_coworker','pending_manager')";
const shiftSelect = "SELECT s.*,w.company_id,w.branch_id,w.department_id,w.week_start,w.timezone,w.state week_state FROM people_roster_shifts s JOIN people_roster_weeks w ON w.id=s.week_id";
function context(database: SqlDatabase, purchase?: {actor: WorkContext; input: RosterCommand; key: string}, coverage?: (companyId: string) => Promise<boolean>): PeopleRosterPorts<SqlStatement> {
    return { repository: { readDepartments(...args) { return rosterOrganizationFacts(database).readDepartments(...args); },
            readWeek(fact0: string, fact1: string) { return database.prepare(ppRd25ByIdCompany('people_roster_weeks')).bind(fact0, fact1).first<RosterWeek>(); },
            readShift(fact0: string, fact1: string) { return database.prepare(`${shiftSelect} WHERE s.id=? AND w.company_id=?`).bind(fact0, fact1).first<ShiftWithWeek>(); },
            readTimes(fact0: string, fact1: string, fact2: string, fact3: string, fact4: string, fact5: string, fact6: string, fact7: string, fact8: string, fact9: string) {
                return database.prepare(`SELECT
    (extract(epoch FROM (?::timestamp AT TIME ZONE ?))*1000)::bigint starts_at,
    (extract(epoch FROM (?::timestamp AT TIME ZONE ?))*1000)::bigint ends_at,
    to_char((?::timestamp AT TIME ZONE ?) AT TIME ZONE ?,'YYYY-MM-DD HH24:MI') local_start,
    to_char((?::timestamp AT TIME ZONE ?) AT TIME ZONE ?,'YYYY-MM-DD HH24:MI') local_end`).bind(fact0, fact1, fact2, fact3, fact4, fact5, fact6, fact7, fact8, fact9).first<{
                    starts_at: number;
                    ends_at: number;
                    local_start: string;
                    local_end: string;
                }>();
            },
            readRosterGaps(...args) { return rosterOrganizationFacts(database).readRosterGaps(...args); },
            async readEscalateRosterCoverage(...args) {
                const departments = await rosterOrganizationFacts(database).readEscalateRosterCoverage(...args);
                if (!coverage) return departments;
                const permitted = [];
                for (const department of departments) if (await coverage(department.company_id)) permitted.push(department);
                return permitted;
            },
            applyEscalateRosterCoverage(...args) { return rosterNotifications(database).applyEscalateRosterCoverage(...args); },
            readAssignmentIssues(fact0: number, fact1: string, fact2: number, fact3: string) {
                return database.prepare("SELECT to_char(to_timestamp(?::bigint/1000.0) AT TIME ZONE ?,'YYYY-MM-DD') first_day,to_char(to_timestamp((?::bigint-1)/1000.0) AT TIME ZONE ?,'YYYY-MM-DD') last_day").bind(fact0, fact1, fact2, fact3).first<{
                    first_day: string;
                    last_day: string;
                }>();
            },
            readAssignmentIssues2(fact0: string, fact1: string, fact2: string, fact3: string) {
                return database.prepare(`SELECT l.id FROM leave_requests l LEFT JOIN employees e ON e.id=l.employee_id AND e.company_id=l.company_id
    WHERE l.company_id=? AND l.state='approved' AND coalesce(l.person_id,e.person_id)=?
    AND EXISTS(SELECT 1 FROM (
 SELECT ds.leave_date::date absence_date FROM leave_day_segments ds WHERE ds.request_id=l.id AND ds.company_id=l.company_id
 UNION ALL SELECT generate_series(l.start_date::date,coalesce(l.end_date::date,l.start_date::date+l.days::int-1),interval '1 day')::date WHERE NOT EXISTS(SELECT 1 FROM leave_day_segments ds WHERE ds.request_id=l.id)
 ) absence WHERE absence_date<=?::date AND absence_date>=?::date) LIMIT 1`).bind(fact0, fact1, fact2, fact3).first<Record<string, unknown>>();
            },
            readAssignmentIssues3(fact0: string, fact1: string, fact2: number, fact3: number, fact4: string) {
                return database.prepare(`SELECT a.id FROM people_roster_assignments a JOIN people_roster_shifts s ON s.id=a.shift_id JOIN people_roster_weeks w ON w.id=s.week_id
    WHERE w.company_id=? AND a.person_id=? AND a.state='assigned' AND s.state='active' AND s.starts_at<? AND s.ends_at>?
    AND NOT (a.id=ANY(?::text[])) LIMIT 1`).bind(fact0, fact1, fact2, fact3, fact4).first<Record<string, unknown>>();
            },
            readCapacity(fact0: string, fact1: string) {
                return database.prepare("SELECT count(*)::int n FROM people_roster_assignments WHERE shift_id=? AND state='assigned' AND NOT(id=ANY(?::text[]))").bind(fact0, fact1).first<{
                    n: number;
                }>();
            },
            applyAssign(fact0: string, fact1: string, fact2: string, fact3: string) { return nativePpStatement('people_roster_stmt_1').bind(fact0, fact1, fact2, fact3).run(); },
            readNotice(fact0: string, fact1: string) {
                return database.prepare(ppRd36TwoPredTail("user_id","hr_people","id","company_id"," AND user_id IS NOT NULL")).bind(fact0, fact1).first<{
                    user_id: string;
                }>();
            },
            applyNotice(...args) { return rosterNotifications(database).applyNotice(...args); },
            readNotifyManagers(...args) { return rosterOrganizationFacts(database).readNotifyManagers(...args); },
            applyNotifyManagers(...args) { return rosterNotifications(database).applyNotifyManagers(...args); },
            applyHistory(fact0: string, fact1: string, fact2: string, fact3: string | null, fact4: string | null, fact5: string, fact6: string, fact7: string, fact8: string, fact9: string, fact10: number) { return nativePpStatement('people_roster_stmt_2').bind(fact0, fact1, fact2, fact3, fact4, fact5, fact6, fact7, fact8, fact9, fact10).run(); },
            async readInvalidate(fact0: number, fact1: string, fact2: string, fact3: string, fact4: string, fact5: string) {
                return (await nativePpStatement('people_roster_stmt_3').bind(fact0, fact1, fact2, fact3, fact4, fact5).all<RosterRequest>()).results;
            },
            readRescheduleRosterShift(...args) { return rosterIdentityFacts(database).readRescheduleRosterShift(...args); },
            applyRescheduleRosterShift(fact0: string) { return nativePpStatement('people_roster_stmt_4').bind(fact0).run(); },
            async readRescheduleRosterShift2(fact0: string) { return (await database.prepare("SELECT * FROM people_roster_assignments WHERE shift_id=? AND state='assigned' FOR UPDATE").bind(fact0).all<RosterAssignment>()).results; },
            applyRescheduleRosterShift2(fact0: string, fact1: string, fact2: string, fact3: number, fact4: number, fact5: string) { return nativePpStatement('people_roster_stmt_5').bind(fact0, fact1, fact2, fact3, fact4, fact5).run(); },
            readRescheduleRosterShift3(fact0: number, fact1: string) { return nativePpStatement('people_roster_stmt_6').bind(fact0, fact1).first<RosterWeek>(); },
            readReadRequest(fact0: string, fact1: string) { return database.prepare(ppRd25ByIdCompany('people_roster_requests')).bind(fact0, fact1).first<RosterRequest>(); },
            readRequestParticipants(fact0: string, fact1: string, fact2: string) { return database.prepare("SELECT 1 FROM hr_people WHERE id=? AND company_id=? AND iam_same_person(user_id,?,company_id)").bind(fact0, fact1, fact2).first<Record<string, unknown>>(); },
            readRequestCheck(fact0: string) { return rosterAssignmentById<RosterAssignment>(database, fact0); },
            readRequestCheck2(fact0: string) { return rosterAssignmentById<RosterAssignment>(database, fact0); },
            applyRosterCommand(fact0: string) { return nativePpStatement('people_roster_stmt_7').bind(fact0).run(); },
            readRosterCommand(...args) { return rosterIdentityFacts(database).readRosterCommand(...args); },
            readRosterCommand2(fact0: string, fact1: string, fact2: string) { return database.prepare(ppRd36ThreePred("*", "people_roster_templates", "id", "company_id", "department_id")).bind(fact0, fact1, fact2).first<ShiftTemplate>(); },
            applyRosterCommand2(fact0: string, fact1: string, fact2: string, fact3: string, fact4: string, fact5: string, fact6: number, fact7: boolean, fact8: number, fact9: string, fact10: number) {
                return nativePpStatement('people_roster_stmt_8').bind(fact0, fact1, fact2, fact3, fact4, fact5, fact6, fact7, fact8, fact9, fact10).run();
            },
            readRosterCommand3(fact0: string) { return database.prepare(ppRd36SinglePred("1", "pg_timezone_names", "name")).bind(fact0).first<Record<string, unknown>>(); },
            readRosterCommand4(fact0: string, fact1: string, fact2: string) { return database.prepare("SELECT 1 FROM people_roster_weeks WHERE company_id=? AND department_id=? AND week_start=?::date").bind(fact0, fact1, fact2).first<Record<string, unknown>>(); },
            applyRosterCommand3(fact0: string, fact1: string, fact2: string | null, fact3: string, fact4: string, fact5: string, fact6: string, fact7: number, fact8: number) { return nativePpStatement('people_roster_stmt_9').bind(fact0, fact1, fact2, fact3, fact4, fact5, fact6, fact7, fact8).run(); },
            async readRosterCommand5(fact0: string) { return (await database.prepare(ppRd58RosterShiftsByWeekSql(" AND state='active' ORDER BY starts_at,id")).bind(fact0).all<RosterShift>()).results; },
            applyRosterCommand4(fact0: string, fact1: string, fact2: string | null, fact3: string, fact4: string, fact5: string, fact6: string, fact7: number, fact8: number, fact9: number) { return nativePpStatement('people_roster_stmt_10').bind(fact0, fact1, fact2, fact3, fact4, fact5, fact6, fact7, fact8, fact9).run(); },
            readRosterCommand6(fact0: string) { return rosterAssignmentsByShift<RosterAssignment>(database, fact0); },
            async readRosterCommand7(fact0: string) { return (await database.prepare(ppRd58RosterShiftsByWeekSql(" AND state='active'")).bind(fact0).all<RosterShift>()).results; },
            readRosterCommand8(fact0: string) { return rosterAssignmentsByShift<RosterAssignment>(database, fact0); },
            applyRosterCommand5(fact0: string, fact1: number, fact2: string) { return nativePpStatement('people_roster_stmt_11').bind(fact0, fact1, fact2).run(); },
            readRosterCommand9(fact0: string, fact1: string, fact2: string) { return database.prepare("SELECT 1 FROM people_roster_templates WHERE id=? AND company_id=? AND department_id=? AND active").bind(fact0, fact1, fact2).first<Record<string, unknown>>(); },
            async readRosterCommand10(fact0: string) { return (await database.prepare("SELECT * FROM people_roster_assignments WHERE shift_id=? AND state='assigned' FOR UPDATE").bind(fact0).all<RosterAssignment>()).results; },
            readRosterCommand11(fact0: string, fact1: string) { return database.prepare("SELECT id FROM hr_people WHERE id=? AND company_id=? FOR UPDATE").bind(fact0, fact1).first<Record<string, unknown>>(); },
            applyRosterCommand6(fact0: string, fact1: string, fact2: string | null, fact3: string, fact4: string, fact5: string, fact6: string, fact7: number, fact8: number, fact9: number, fact10: number) {
                return nativePpStatement('people_roster_stmt_12').bind(fact0, fact1, fact2, fact3, fact4, fact5, fact6, fact7, fact8, fact9, fact10).run();
            },
            applyRosterCommand7(fact0: string) { return nativePpStatement('people_roster_stmt_13').bind(fact0).run(); },
            applyRosterCommand8(fact0: number, fact1: string) { return nativePpStatement('people_roster_stmt_14').bind(fact0, fact1).run(); },
            async readRosterCommand12(fact0: string) { return (await database.prepare("SELECT * FROM people_roster_assignments WHERE shift_id=? AND state='assigned' FOR UPDATE").bind(fact0).all<RosterAssignment>()).results; },
            applyRosterCommand9(fact0: string) { return nativePpStatement('people_roster_stmt_15').bind(fact0).run(); },
            applyRosterCommand10(fact0: string) { return nativePpStatement('people_roster_stmt_16').bind(fact0).run(); },
            readRosterCommand13(fact0: number, fact1: string) { return nativePpStatement('people_roster_stmt_17').bind(fact0, fact1).first<RosterWeek>(); },
            readRosterCommand14(fact0: string) { return rosterAssignmentById<RosterAssignment>(database, fact0); },
            readRosterCommand15(fact0: string) { return rosterAssignmentById<RosterAssignment>(database, fact0); },
            readRosterCommand16(fact0: string, fact1: string, fact2: string) { return database.prepare(`SELECT 1 FROM people_roster_requests WHERE company_id=? AND requester_person_id=? AND target_shift_id=? AND state IN ${pending}`).bind(fact0, fact1, fact2).first<Record<string, unknown>>(); },
            applyRosterCommand11(fact0: string, fact1: string, fact2: string, fact3: "extra" | "move" | "swap", fact4: string, fact5: string | null, fact6: string | null, fact7: number | null, fact8: string, fact9: string | null, fact10: number | null, fact11: "pending_coworker" | "pending_manager" | "approved" | "rejected" | "withdrawn" | "invalidated", fact12: string, fact13: string, fact14: number, fact15: number) { return nativePpStatement('people_roster_stmt_18').bind(fact0, fact1, fact2, fact3, fact4, fact5, fact6, fact7, fact8, fact9, fact10, fact11, fact12, fact13, fact14, fact15).run(); },
            async readRosterCommand17(fact0: string) { return (await database.prepare("SELECT id FROM people_roster_shifts WHERE id=ANY(?::text[]) ORDER BY id FOR UPDATE").bind(fact0).all<Record<string, unknown>>()).results; },
            applyRosterCommand12(fact0: string) { return nativePpStatement('people_roster_stmt_19').bind(fact0).run(); },
            applyRosterCommand13(fact0: number, fact1: string) { return nativePpStatement('people_roster_stmt_20').bind(fact0, fact1).run(); },
            applyRosterCommand14(fact0: "pending_manager" | "approved" | "rejected" | "withdrawn", fact1: string, fact2: "accept" | "approve" | "reject" | "withdraw", fact3: string, fact4: string, fact5: number, fact6: string) { return nativePpStatement('people_roster_stmt_21').bind(fact0, fact1, fact2, fact3, fact4, fact5, fact6).run(); },
            async readRosterData(fact0: string, fact1: string, fact2: boolean, fact3: string, fact4: string) { return (await database.prepare("SELECT * FROM people_roster_weeks WHERE company_id=? AND department_id=? AND (? OR state='published') AND week_start BETWEEN ?::date AND ?::date ORDER BY week_start").bind(fact0, fact1, fact2, fact3, fact4).all<RosterWeek>()).results; },
            async readRosterData2(fact0: string) { return (await database.prepare("SELECT * FROM people_roster_shifts WHERE week_id=ANY(?::text[]) ORDER BY starts_at,name,id").bind(fact0).all<RosterShift>()).results; },
            async readRosterData3(fact0: string) { return (await database.prepare(`SELECT a.*,p.legal_name person_name FROM people_roster_assignments a JOIN hr_people p ON p.id=a.person_id JOIN people_roster_shifts s ON s.id=a.shift_id WHERE s.week_id=ANY(?::text[]) AND a.state='assigned' ORDER BY p.legal_name`).bind(fact0).all<RosterAssignment>()).results; },
            async readRosterData4(fact0: string, fact1: string) { return (await database.prepare(ppRd36TwoPredTail("*","people_roster_templates","company_id","department_id"," ORDER BY start_time,name")).bind(fact0, fact1).all<ShiftTemplate>()).results; },
            async readRosterData5(fact0: string, fact1: string, fact2: boolean, fact3: string | null, fact4: string | null) {
                return (await database.prepare(`SELECT r.*,p.legal_name requester_name,c.legal_name coworker_name FROM people_roster_requests r JOIN hr_people p ON p.id=r.requester_person_id LEFT JOIN hr_people c ON c.id=r.coworker_person_id
    WHERE r.company_id=? AND r.department_id=? AND (? OR r.requester_person_id=? OR r.coworker_person_id=?) ORDER BY r.created_at DESC LIMIT 200`).bind(fact0, fact1, fact2, fact3, fact4).all<RosterRequest>()).results;
            },
            readRosterData6(fact0: string) { return database.prepare("SELECT a.*,p.legal_name person_name FROM people_roster_assignments a JOIN hr_people p ON p.id=a.person_id WHERE a.id=?").bind(fact0).first<RosterAssignment>(); },
            async readRosterData7(fact0: string, fact1: string, fact2: boolean, fact3: string | null, fact4: string, fact5: string | null, fact6: string | null) {
                return (await database.prepare(`SELECT h.id,h.record_id,h.action,h.reason,h.created_at,COALESCE(p.legal_name,'Roster manager') actor_name
    FROM people_roster_history h LEFT JOIN hr_people p ON p.user_id=h.actor_id AND p.company_id=h.company_id AND p.merged_into_id IS NULL
    WHERE h.company_id=? AND h.department_id=? AND ((? AND (h.week_id=? OR h.request_id IS NOT NULL)) OR h.request_id IN
    (SELECT id FROM people_roster_requests WHERE company_id=? AND (requester_person_id=? OR coworker_person_id=?))) ORDER BY h.created_at DESC LIMIT 60`).bind(fact0, fact1, fact2, fact3, fact4, fact5, fact6).all<RosterHistory>()).results;
            } }, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, today: () => new Intl.DateTimeFormat('en-CA', { timeZone: 'Asia/Bangkok', year: 'numeric', month: '2-digit', day: '2-digit' }).format(new Date()), commands: { replay: (...args) => replayCommand(database, ...args), commit: (...args) => executeCommand(database, ...args) }, activeStaff: (...args) => fromNative(() => activeStaff(database, ...args)), rosterStaffPeople: (...args) => rosterStaffPeople(database, ...args), transaction: run => {
            if (!isTransactionalSqlDatabase(database))
                throw new Error('ROSTER_TRANSACTION_REQUIRED');
            return purchase ? withRosterPurchasedTransaction(database, purchase.actor, {key: purchase.key, operation: `roster.${purchase.input.action}`, purpose: rosterPurchasePurpose(purchase.input)}, tx => run(context(tx)))
                : database.transaction(tx => run(context(tx)));
        } };
}
async function fromNative<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof ApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function invoke<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function escalateRosterCoverage(db: SqlDatabase, today = rosterMonday()) {
    return withRosterCoveragePurchasedTransaction(db, (transaction, permitsCompany) => invoke(() => createPeopleRosterApplication(context(transaction, undefined, permitsCompany)).escalateRosterCoverage(today)));
}
export async function rescheduleRosterShift(db: SqlDatabase, actor: WorkContext, input: {
    id: string;
    expectedVersion: number;
    date: string;
    startTime: string;
    endTime: string;
    reason: string;
}) { return invoke(() => createPeopleRosterApplication(context(db)).rescheduleRosterShift(actor, input)); }
export async function rosterCommand(database: PostgresDatabase, actor: WorkContext, raw: unknown, key: string) {
    const parsed = rosterCommandSchema.safeParse(raw);
    return invoke(() => createPeopleRosterApplication(context(database, parsed.success ? {actor, input: parsed.data, key} : undefined)).rosterCommand(actor, raw, key));
}
export async function rosterData(db: SqlDatabase, actor: WorkContext, query: {
    department?: string;
    week?: string;
} = {}) {
    const purchase = await rosterPurchasedAccess(db, actor);
    const data = await invoke(() => createPeopleRosterApplication(context(db)).rosterData(actor, query));
    if (purchase) {
        data.purchase = purchase;
        if (!purchase.settleExisting) for (const request of data.requests) request.permittedActions = [];
    }
    return data;
}
export async function rosterNotificationLink(db: SqlDatabase, actor: WorkContext, type: string, id: string) {
    await rosterPurchasedAccess(db, actor);
    return invoke(() => createPeopleRosterApplication(context(db)).rosterNotificationLink(actor, type, id));
}
