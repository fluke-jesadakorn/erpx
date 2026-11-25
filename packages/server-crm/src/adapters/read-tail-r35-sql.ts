/** G4 / retire.crm.read-tail-r35 byte-identical read heads.
 *
 * Each renderer owns exactly one read token for its family; callers pass their
 * own projection, alias and tail so rendered SQL and bind order stay
 * byte-identical to the retired inline literals. Locks, command-receipt reads,
 * native command kinds and the r8-r34 families are untouched.
 */

/** Contact-with-account workspace read head shared by the identities contact
 * list and the single-contact read. Callers pass the contact projection, the
 * account alias (the scope fragments bind to the contact alias c) and the tail. */
import { crmProjectionFromSql } from './read-tail-r46-sql';

export const crmContactAccountReadSql = (projection: string, accountAlias: string, tail: string) =>
  crmProjectionFromSql(projection, `crm_contacts c JOIN crm_accounts ${accountAlias} ON ${accountAlias}.id=c.account_id ${tail}`);

/** Assignment-module membership EXISTS predicate shared by the metric bare
 * guard and the aliased prefix used by metric/analytics purchases. Callers pass
 * the optional assignment alias and the suffix after jsonb_build_array(. */
export const crmAssignmentModuleExistsSql = (alias: string, suffix: string) => {
  const q = alias ? `${alias}.` : '';
  return `EXISTS(SELECT 1 FROM assignments${alias ? ` ${alias}` : ''} WHERE ${q}id=? AND (jsonb_array_length(${q}modules_json::jsonb)=0 OR ${q}modules_json::jsonb @> jsonb_build_array(${suffix}`;
};
