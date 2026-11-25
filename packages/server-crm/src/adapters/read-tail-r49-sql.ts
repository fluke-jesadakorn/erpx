/** G4 / retire.crm.read-tail-r49 byte-identical JSONB-aggregate head.
 *
 * The native-image witness and the related-image aggregate share one read head:
 * both project the JSONB-aggregate prefix first, then append their own body.
 * The renderer below reproduces the pre-dedupe inline SQL byte-for-byte; callers
 * pass only their own body, so rendered SQL, whitespace and bind order stay
 * identical. Locks, no-space probes, command-receipt reads, native command/read
 * kinds, cross-owner exports and the r8-r48 families are untouched.
 */
export const crmJsonbAggregateSql = (body: string) => `SELECT jsonb_agg(to_jsonb(${body}`;
