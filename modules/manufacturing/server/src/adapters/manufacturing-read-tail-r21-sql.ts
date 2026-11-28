/** Byte-identical Manufacturing read-tail r21 SQL text shared across the
 * attachment-source image projections. Text only: every caller still prepares
 * and executes its own statement with its own projection, source rows, joins
 * and binds; nothing is cached or executed here and it stays outside the RD_*
 * read registry. Each argument re-emits the previously inline SQL byte for
 * byte, including the leading newline of each optional FROM/join tail. */
export const manufacturingAttachmentImageSelect = (
  projection: string,
  from: string,
  joins = "",
) =>
  `SELECT ${projection}::text image${from} record${joins} WHERE record.id=? AND record.company_id=?`;
