import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeCalendarRead } from '@erpx/work-projection-ui/lib/native-calendar-read';
/** Both calendar-index reads are owner-native. The mounted axum
 * `/work/owner-read` route owns the byte-identical SQL, Company scope and
 * response shape; no TypeScript SQL or fallback remains for them. */
export function calendarIndexWorkProjectionFacts(db: SqlDatabase) {
    return { read2: (facts: {
            value1: string;
            value2: string;
            value3: string;
        }) => nativeCalendarRead.read68(facts),
        read5: (facts: {
            value1: string;
            value2: string;
        }) => nativeCalendarRead.read69(facts) };
}
