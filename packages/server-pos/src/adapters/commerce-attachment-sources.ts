import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {posCatalogAggregateHeadSql} from './read-tail-r44-catalog-aggregate-sql';
/** Original generic product-image fallback remains POS-owned. This private
 * witness exposes no catalog or register-management permission. */
export function commerceAttachmentPosSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE pos_catalog,pos_register_catalog,pos_registers IN SHARE ROW EXCLUSIVE MODE');}
export function commerceAttachmentPosSourceImage(database:SqlDatabase,companyId:string,productId:string){return database.prepare(`SELECT jsonb_build_object('catalog',(${posCatalogAggregateHeadSql()}product_id=? AND c.company_id=?),'registers',(SELECT jsonb_agg(jsonb_build_object('selection',to_jsonb(s),'register',to_jsonb(r)) ORDER BY s.register_id) FROM pos_register_catalog s JOIN pos_registers r ON r.id=s.register_id AND r.company_id=? WHERE s.product_id=?))::text image`).bind(productId,companyId,companyId,productId).first<string>('image');}
