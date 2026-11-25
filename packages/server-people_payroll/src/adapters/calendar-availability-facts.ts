import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {
  nativeCalendarAvailabilityLeaveDays,
  type LaneHoldingScope,
} from "./native-lane-6/client";

/**
 * Owner-local calendar-availability leave-day facts.
 *
 * The retired company-scoped expansion query now lives in the native lane-6
 * axum/sqlx owner. This adapter only transports bounded Company, person and
 * date facts; it emits no SQL and has no fallback reader. When the caller
 * supplies its WorkContext holding scope the native transport selects that
 * exact registered Holding; otherwise it selects the one Holding registered
 * for the Company. There is never a first-holding fallback.
 */
export function calendarAvailabilityPeoplePayrollFacts(
    _db: SqlDatabase,
    scope?: LaneHoldingScope,
) {
    return {
        read7: (facts: {
            value1: string;
            value2: string[];
            value3: string;
            value4: string;
        }) => nativeCalendarAvailabilityLeaveDays(
            scope,
            facts.value1,
            facts.value2,
            facts.value3,
            facts.value4,
        ),
    };
}
