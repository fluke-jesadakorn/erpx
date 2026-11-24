import type {betaManagementPeopleSources} from '@erpx/server-people_payroll/public/beta-management-sources';

type MemberSources = Pick<
  ReturnType<typeof betaManagementPeopleSources>,
  'memberCompanySource' | 'memberCompanyPoolSource'
>;
export type BetaMemberCountDialect = 'bound' | 'positional';

/** One parameterized beta_members count head shared by the Postgres pool
 * capacity query and the SQLite capacity guard. The caller supplies only its
 * trusted owner source fragments; the projection head stays here and places
 * the matching fragment, so neither reader can drift. No request text is interpolated. */
export const betaMemberCountSql = (sources: MemberSources, dialect: BetaMemberCountDialect) =>
  `SELECT count(*) FROM beta_members m ${dialect === 'bound' ? sources.memberCompanySource : sources.memberCompanyPoolSource}`;
