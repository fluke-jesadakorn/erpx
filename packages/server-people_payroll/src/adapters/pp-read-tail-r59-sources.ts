import 'server-only';
import { ppRdRound2cStarByOnePred } from './pp-read-tail-round2c-sources';

/** Canonical parameterized read builder (G4 read-tail r59). The former owner-local
 * Thai payroll profile head now renders from the shared round-2c template; this
 * builder pins its exact table/column and the callers keep their predicate, ORDER
 * BY tail, projection and bind arity unchanged. Pure read only. */

/** Thai payroll profiles by company list. `tail` is the exact original
 * predicate/ordering tail; the fixed head, projection and bind arity stay
 * byte-identical. */
export const ppRd59ThaiProfilesByCompanySql = (tail: string) =>
  ppRdRound2cStarByOnePred('hr_thai_payroll_profiles', 'company_id', tail);
