/** Owner-local attachment source projection shared by the Quality hold and
 * specification readers. The four parameterized parts (branch nullability,
 * kind literal, image witness, source relation) are the only differences;
 * the statement head, `NULL::text department_id` column and trailing
 * `WHERE id=? AND company_id=?` binds are byte-identical to the former
 * inline copies. Both binds remain id then company_id in that order. */
export const qualityAttachmentSourceSelect = (branch: string, kind: string, image: string, from: string) =>
  `SELECT id,company_id,${branch},NULL::text department_id,${kind},${image} FROM ${from} WHERE id=? AND company_id=?`;
