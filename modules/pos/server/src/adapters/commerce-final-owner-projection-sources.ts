import "server-only";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commerceFinalOwnerProjectionSources = {
  pos_catalog: "SELECT \"product_id\",\"company_id\",\"barcode\",\"category\",\"kind\",\"station\",\"recipe_json\",\"modifiers_json\",\"tax_inclusive\",\"version\",\"enabled\",\"image_id\" FROM pos_catalog",
  pos_register_catalog: "SELECT \"register_id\",\"product_id\",\"enabled\" FROM pos_register_catalog",
  pos_registers: "SELECT \"id\",\"company_id\",\"branch_id\",\"warehouse_id\",\"name\",\"mode\",\"active\",\"version\" FROM pos_registers",
} as const;
