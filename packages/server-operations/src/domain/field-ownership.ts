/** Transitional dispatch ownership only. Mixed policies keep their existing
 * consumers; Field-only policy writes belong to the native Field contract. */
export function fieldOnlyPolicy(kinds:unknown):boolean{return Array.isArray(kinds)&&kinds.length>0&&kinds.every(kind=>kind==='field_service_order');}
