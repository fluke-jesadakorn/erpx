import 'server-only';
import { ppRdRound2bNotExistsByOnePred } from './pp-read-tail-round2b-sources';

/** Canonical parameterized read builder (G4 read-tail r62). The former owner-local
 * NOT EXISTS head now renders from the shared round-2b template; this builder
 * pins its original table and predicate column and passes the caller's tail
 * unchanged, so every rendered guard stays byte-identical. Pure read only. */

/** People email-uniqueness guard. `tail` is the exact original predicate tail;
 * the fixed head, closing parenthesis and bind arity stay byte-identical. */
export const ppRd62PeopleEmailGuard = (tail: string) =>
  ppRdRound2bNotExistsByOnePred('hr_people', 'company_id', tail);
