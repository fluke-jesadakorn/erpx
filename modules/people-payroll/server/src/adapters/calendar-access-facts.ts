import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { CalendarPerson } from "@erpx/work-projection-ui/lib/calendar";
import { nativeCalendarAccessPeople, type LaneHoldingScope } from "./native-lane-6/client";

/**
 * Owner-local calendar-access people facts. The retired Company-scoped people
 * list (joining hr_people / hr_engagements with the owner lifecycle probe) now
 * lives in the native lane-6 axum/sqlx owner. This adapter transports only
 * bounded identities and emits no SQL for it, with no fallback reader.
 */
export function calendarAccessPeoplePayrollFacts(
    _db: SqlDatabase,
    scope?: LaneHoldingScope,
) {
    return {
        people: (input: {
            companyId: string;
            peopleIds: string[];
            lifecycleReader: boolean;
            search: string;
            take: number;
            cursor: number;
        }): Promise<CalendarPerson[]> => nativeCalendarAccessPeople(
            scope,
            input.companyId,
            input.peopleIds,
            input.lifecycleReader,
            input.search,
            input.take,
            input.cursor,
        ),
    };
}
