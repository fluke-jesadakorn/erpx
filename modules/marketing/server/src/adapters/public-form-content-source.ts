import { formAssetReadHead } from './form-asset-read-head';

/** Canonical owner-local fixed-SQL for the active Marketing form-content source.
 * The presentation assertion and the content reader share this one statement,
 * so no duplicate SQL literal remains. Callers supply only the projection;
 * the owner-owned source predicate stays in the shared form/content head. */
export const marketingFormContentSql = (projection: string): string =>
  formAssetReadHead({ kind: 'content', projection });
