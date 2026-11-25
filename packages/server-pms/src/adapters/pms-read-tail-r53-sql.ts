/** Byte-identical PMS read-tail r53 owner projection head shared by the bare
 * pms_postings projection and the wrapped `FROM (...) pms_postings` fragment.
 * Text only: every consumer keeps its own wrapper, binds, result type and
 * transaction; nothing is cached or executed here. The kind is a closed union
 * owned by this module, never caller SQL. Locking reads, the native registry
 * and the r17-r52 fragments stay untouched. */

/** Closed bare pms_postings head with explicit owned columns. */
type Head = 'postings';
const HEAD: Record<Head, string> = {
  postings: 'SELECT "id","property_id","source_key","record_id","journal_id","business_date" FROM pms_postings',
};
export const pmsOwnerProjectionHeadSql = (kind: Head): string => HEAD[kind];
