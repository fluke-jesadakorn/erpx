import 'server-only';
/** Byte-identical catalog aggregate head shared by the commerce attachment
 * image read and the POS startup source read. Each caller keeps its own
 * WHERE tail, so rendered SQL, binds and locks stay byte-identical. */
export const posCatalogAggregateHeadSql = (): string => 'SELECT jsonb_agg(to_jsonb(c) ORDER BY c.product_id) FROM pos_catalog c WHERE c.';
