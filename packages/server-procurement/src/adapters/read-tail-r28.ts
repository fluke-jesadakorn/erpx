/** Procurement read-tail r28 shared fragment. Read-only text only: the builder
 * renders the original statement so binds, ordering, locks and writes are
 * unchanged. The r17-r27 helpers stay untouched. */
/** Recursive attachment-version witness shared by the supply package-obligations
 * RFQ and supplier-bill branches. `alias` is the outer record alias and
 * `targetType` is the record_attachment_version target label. */
export const attachmentVersionWitness = (
  steps: string,
  alias: string,
  targetType: string,
) =>
  `  AND EXISTS(WITH RECURSIVE versions(version) AS(SELECT event.version UNION SELECT binding.next_version FROM versions
   JOIN (${steps}) binding ON binding.previous_version=versions.version AND binding.company_id=${alias}.company_id
   AND binding.target_type='${targetType}' AND binding.target_id=${alias}.id WHERE binding.next_version<=${alias}.version)
   SELECT 1 FROM versions WHERE version=${alias}.version)`;
