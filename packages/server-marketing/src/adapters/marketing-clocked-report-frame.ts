import 'server-only';
import {marketingClockBody} from './materialized-clock-body-sql';

/** Owner-local MATERIALIZED clocked report frame shared by Marketing's
 * purchase-scoped report readers. Callers pass their exact clock alias, gap,
 * projection, source and row alias; the single admitted head keeps every
 * statement byte-identical while the tail (scope, authority, PMS touch and
 * EXISTS wrapper) stays at the call site. */
export const marketingClockedReportFrameSql = (
  clock: string,
  gap: '' | ' ',
  projection: string,
  source: string,
  alias: string,
): string => `WITH ${clock} AS MATERIALIZED${marketingClockBody(gap)} SELECT ${projection} FROM (${source}) ${alias} CROSS JOIN ${clock} WHERE `;
