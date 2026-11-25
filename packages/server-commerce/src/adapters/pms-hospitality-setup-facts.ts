import { commerceFinalOwnerProjectionSources as posProjectionSources } from "@erpx/server-pos/public/commerce-final-owner-projection-sources";
import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as inventoryProjectionSources } from "@erpx/server-inventory/public/commercial-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {all,one} from '@erpx/server-runtime/public/runtime/sql';
import { nativeCommerceReadAll, RD_WAREHOUSE_BRANCH_ACTIVE } from './native-commerce-read';
export function pmsHospitalitySetupCommerceFacts(db: SqlDatabase) {
    return {
        commerce: (facts: {
            company: string;
        }) => one(db, `SELECT s.currency,s.chart_id,c.base_currency chart_currency,c.status chart_status FROM commerce_settings s LEFT JOIN (${financeProjectionSources.finance_charts}) c ON c.id=s.chart_id AND c.company_id=s.company_id WHERE s.company_id=?`, facts.company),
        warehouses: (facts: {
            company: string;
            branch: string;
        }) => nativeCommerceReadAll<{ id: string; name: string }>(RD_WAREHOUSE_BRANCH_ACTIVE, [facts.company, facts.branch], db),
        products: (facts: {
            company: string;
        }) => all(db, `SELECT p.id,p.name,p.sku,p.unit,p.price_minor,p.tax_bps,p.version,c.kind,c.station,c.recipe_json,COALESCE(c.version,0) catalog_version FROM commerce_products p LEFT JOIN (${posProjectionSources.pos_catalog}) c ON c.product_id=p.id WHERE p.company_id=? AND p.status='active' ORDER BY p.name`, facts.company),
        stock: (facts: {
            company: string;
            branch: string;
        }) => one(db, `SELECT COUNT(*) n FROM (${inventoryProjectionSources.commerce_stock}) s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=? AND w.branch_id=? AND s.quantity_milli>0`, facts.company, facts.branch)
    };
}
