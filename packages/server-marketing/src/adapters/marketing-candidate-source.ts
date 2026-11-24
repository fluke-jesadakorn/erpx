import 'server-only';
import {marketingPlainSourceSql} from './marketing-plain-source-sql';
/** Canonical owner-local fixed source for the unenforced/legacy Marketing
 * candidate read. The analytics legacy report and the metric fallback share
 * this one admitted projection; the plain-source frame also serves the package
 * settlement readers, so no duplicate TS SQL literal remains in the owner.
 * Callers supply no SQL and no relation name. */
export const marketingCandidateSourceSql = marketingPlainSourceSql('candidate');
