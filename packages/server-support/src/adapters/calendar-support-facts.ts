import "server-only";
import { nativeSupportRuntime } from "@erpx/server-support/adapters/command-runtime";
import {
  nativeSupportReadAll,
  SUPPORT_RD_CALENDAR_SUPPORT_INCIDENTS,
  SUPPORT_RD_CALENDAR_SUPPORT_TICKETS,
} from "@erpx/server-support/adapters/native-support-read";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

/**
 * Support Calendar facts are now planned by the native Support `command-plan`
 * route. The authoritative reads stay on the Support-owned `support_tickets`
 * and `support_incidents`; the actor read predicate and projected owner/team
 * columns are fixed native SQL. Only scalar scope values cross the wire and
 * the planned statement runs on the caller's already-open unit.
 */
export function calendarSupportSupportFacts(db: SqlDatabase) {
  return { read1: async (facts: {
          value1: string;
          value2: string;
          value3: string | null;
          value4: string | null;
          value5: string;
          value6: string;
          value7: string;
          value8: "mine" | "team";
          value9: string;
          value10: string | null;
          value11: string;
          value12: number;
          actor: WorkContext;
      }) => { if (!(await nativeSupportRuntime(facts.actor, db).purchase("support.tickets")).history) return [];
        const a = facts.actor;
        return (await nativeSupportReadAll<{
            id: string;
            title: string;
            due_date: string;
            status: string;
            version: number;
            branch_id: string;
            department_id: string | null;
            owner_name: string | null;
            sla_due_at: number | null;
            source_record_json: string | null;
        }>(SUPPORT_RD_CALENDAR_SUPPORT_TICKETS, [
          facts.value1, facts.value2, facts.value3, facts.value3, facts.value4, facts.value4,
          facts.value5, a.assignmentId, a.userId, a.role,
          facts.value6, facts.value7, facts.value8, facts.value9, facts.value9,
          facts.value10, facts.value10, facts.value11, facts.value11, facts.value12,
        ], db)).results; },
      read2: async (facts: {
          value1: string;
          value2: string | null;
          value3: string | null;
          value4: string;
          value5: string;
          value6: string;
          value7: "mine" | "team";
          value8: string;
          value9: string | null;
          value10: string;
          value11: number;
          actor: WorkContext;
      }) => { if (!(await nativeSupportRuntime(facts.actor, db).purchase("support.incidents")).history) return [];
        const a = facts.actor;
        return (await nativeSupportReadAll<{
            id: string;
            title: string;
            due_date: string;
            status: string;
            version: number;
            branch_id: string;
            department_id: string | null;
        }>(SUPPORT_RD_CALENDAR_SUPPORT_INCIDENTS, [
          facts.value1, facts.value2, facts.value2, facts.value3, facts.value3, facts.value4,
          a.assignmentId, a.userId, a.role,
          facts.value5, facts.value6, facts.value7, facts.value8, facts.value8,
          facts.value9, facts.value9, facts.value10, facts.value10, facts.value11,
        ], db)).results; } };
}
