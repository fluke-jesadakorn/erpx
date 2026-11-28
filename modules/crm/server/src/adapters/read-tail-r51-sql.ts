/** G4 / retire.crm.read-tail-r51 byte-identical lead-select head.
 *
 * The creator-scope EXISTS guard and the eligible-owner probe select a witness
 * row from `crm_leads`. That head is the r42 select-one renderer applied to
 * the fixed body `crm_leads <tail>`, so the head is folded into the owner's
 * single select-one renderer: this module now carries no SQL token of its own
 * and its rendered text is byte-identical to the retired inline literals.
 * Locks, no-space probes, command-receipt reads, native command/read kinds and
 * cross-owner exports remain untouched.
 */
import { crmSelectOneSql } from './read-tail-r42-sql';

export const crmLeadSelectSql = (tail: string) => crmSelectOneSql(`crm_leads ${tail}`);
