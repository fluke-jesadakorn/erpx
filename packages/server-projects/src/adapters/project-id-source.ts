/** Byte-identical point read shared by the Project task product-entry and
 * handoff project lookups. Only the source table varies; projection, bind
 * order and emitted SQL stay exact. */
export const projectIdSql=(table:string)=>`SELECT project_id FROM ${table} WHERE id=? AND company_id=?`;
