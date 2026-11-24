/** Byte-identical scan-pending witness shared by owner read-tail adapters.
 * Text only: every call site still prepares and executes its own statement in
 * its supplied transaction with its own binds; the rendered SQL is unchanged.
 * No SQL text is accepted from a caller and no result is cached here. */
export const scanPendingSql = (attachmentRef: string) =>
  `EXISTS(SELECT 1 FROM attachment_scan_attempts scan WHERE scan.attachment_id=${attachmentRef}.id AND scan.state IN ('pending','running','error'))`;
