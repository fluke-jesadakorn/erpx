import "server-only";
import {packageClockSql} from '@erpx/server-commercial/public/purchased-capability-sql';

/** Owner-local MATERIALIZED clock CTE body shared by Marketing purchase-scoped
 * reads. `gap` is each caller's exact spacing after MATERIALIZED, so every
 * admitted statement keeps its original text byte for byte. */
export const marketingClockBody = (gap: '' | ' '): string => `${gap}(SELECT ${packageClockSql} now_ms)`;
