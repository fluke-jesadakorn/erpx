import { commercialOwnerProjectionSources as inventoryProjectionSources } from "@erpx/server-inventory/public/commercial-owner-projection-sources";
/** Fixed owner facts used by native joined Inventory projections; application ports return typed rows. */
export const inventoryReadProjection = {
    countWarehouse: "JOIN commerce_warehouses w ON w.id=c.warehouse_id",
    countSettings: "JOIN commerce_settings s ON s.company_id=c.company_id",
    lineProduct: "JOIN commerce_products p ON p.id=l.product_id",
    lineStock: `LEFT JOIN (${inventoryProjectionSources.commerce_stock}) s ON s.product_id=l.product_id AND s.warehouse_id=?`,
    lotWarehouse: "JOIN commerce_warehouses w ON w.id=b.warehouse_id",
    transferSource: "JOIN commerce_warehouses s ON s.id=t.source_warehouse_id",
    transferTarget: "JOIN commerce_warehouses d ON d.id=t.target_warehouse_id",
    transferSettings: "JOIN commerce_settings c ON c.company_id=t.company_id",
    transitSource: "JOIN commerce_warehouses s ON s.id=d.source_warehouse_id",
    transitTarget: "JOIN commerce_warehouses w ON w.id=d.target_warehouse_id",
    policyWarehouseScoped: "JOIN commerce_warehouses w ON w.id=r.warehouse_id AND w.company_id=r.company_id",
    policyProduct: "JOIN commerce_products p ON p.id=r.product_id AND p.company_id=r.company_id",
    policyWarehouse: "JOIN commerce_warehouses w ON w.id=r.warehouse_id",
    landedReceipt: "JOIN commerce_fulfillments f ON f.id=c.fulfillment_id",
    receiptOrder: "JOIN commerce_orders o ON o.id=f.order_id",
    orderWarehouse: "JOIN commerce_warehouses w ON w.id=o.warehouse_id",
    receiptMovementLine: "JOIN commerce_fulfillment_lines fl ON fl.fulfillment_id=?",
    receiptMovementProduct: "JOIN commerce_order_lines ol ON ol.id=fl.order_line_id AND ol.product_id=m.product_id",
    receiptMovementSource: "JOIN commerce_fulfillment_lines source_line ON source_line.id=?",
    receiptMovementTime: "JOIN commerce_fulfillments source ON source.id=source_line.fulfillment_id",
    postingSettings: "FROM commerce_settings s",
    purchaseReceipt: "FROM commerce_fulfillments f",
    orderSettings: "JOIN commerce_settings s ON s.company_id=o.company_id"
} as const;
