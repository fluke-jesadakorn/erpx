/** G4 / retire.crm.read-tail-r35 byte-identical read heads.
 *
 * Both former read heads now delegate to owner renderers in r46 and r42, so
 * this module carries no SQL token of its own. Callers still pass their own
 * projection, alias, wrapper and suffix, and rendered SQL and bind order stay
 * byte-identical to the retired inline literals. Locks, command-receipt reads,
 * native command kinds and the r8-r34 families are untouched.
 */
import { crmSelectOneSql } from './read-tail-r42-sql';
import { crmProjectionFromSql } from './read-tail-r46-sql';

/** Contact-with-account workspace read head shared by the identities contact
 * list and the single-contact read. Callers pass the contact projection, the
 * account alias (the scope fragments bind to the contact alias c) and the tail. */
export const crmContactAccountReadSql = (projection: string, accountAlias: string, tail: string) =>
  crmProjectionFromSql(projection, `crm_contacts c JOIN crm_accounts ${accountAlias} ON ${accountAlias}.id=c.account_id ${tail}`);

/** Assignment-module membership EXISTS predicate shared by the metric bare
 * guard and the aliased prefix used by metric/analytics purchases. The inner
 * select-one plan comes from r42; the EXISTS wrapper and caller-supplied suffix
 * stay verbatim. */
export const crmAssignmentModuleExistsSql = (alias: string, suffix: string) => {
  const q = alias ? `${alias}.` : '';
  return `EXISTS(${crmSelectOneSql(`assignments${alias ? ` ${alias}` : ''} WHERE ${q}id=? AND (jsonb_array_length(${q}modules_json::jsonb)=0 OR ${q}modules_json::jsonb @> jsonb_build_array(${suffix}`)}`;
};
