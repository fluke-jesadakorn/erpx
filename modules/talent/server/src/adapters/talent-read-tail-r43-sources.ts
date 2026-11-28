import 'server-only';
import {existsGuardPrefix} from './talent-read-tail-r27-sources';
/** Owner-local EXISTS source-witness head family (round 43). The row source
 * and trailing WHERE predicate stay at the attachment-read caller; the emitted
 * SQL stays byte-identical to the former inline offer witness. The r27 EXISTS
 * prefix is reused so this family adds no second read head. Text only:
 * nothing is cached or executed here. */
export const existsSourceWitness = (source: string, predicate: string) => `${existsGuardPrefix(source)}${predicate})`;
