/** G4 / retire.crm.read-tail-r49 byte-identical JSONB-aggregate head.
 *
 * The aggregate prefix is the owner select-body plan applied to the body
 * `jsonb_agg(to_jsonb(${body}`, so this module no longer carries a read token.
 * Both attachment consumers keep their aggregate body, wrapper and bind order
 * byte-identically. Locks, no-space probes, command-receipt reads, native
 * command/read kinds, cross-owner exports and the r8-r48 families are untouched.
 */
import { crmSelectBodySql } from './read-tail-r46-sql';

export const crmJsonbAggregateSql = (body: string) => crmSelectBodySql(`jsonb_agg(to_jsonb(${body}`);
