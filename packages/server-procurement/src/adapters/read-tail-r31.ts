/** Procurement read-tail r31 shared fragments. Read-only text only: each builder
 * renders the exact original statement so binds, ordering, locks and writes are
 * unchanged. r17-r30 helpers, FOR UPDATE/FOR SHARE and command_receipts stay untouched. */
/** Attachment source image reads share one image/FROM/WHERE projection shape.
 * The image expression (including its ::text image cast), FROM source and
 * WHERE predicate render verbatim. */
export const sourceImage = (image: string, from: string, where: string) =>
  `SELECT ${image} FROM ${from} WHERE ${where}`;
