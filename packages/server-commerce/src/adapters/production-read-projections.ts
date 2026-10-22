/** Owner-native joined facts retain the caller's single SQL snapshot and transaction. */ export const productionReadProjection = {
    "outputProduct": "JOIN commerce_products p ON p.id=s.output_product_id",
    "materialProduct": "JOIN commerce_products p ON p.id=l.product_id",
    "reservationProduct": "JOIN commerce_products p ON p.id=r.product_id",
    "executiveOutput": "LEFT JOIN commerce_products output ON output.id=spec.output_product_id AND output.company_id=s.company_id",
    "executiveWarehouse": "LEFT JOIN commerce_warehouses w ON w.id=s.warehouse_id AND w.company_id=s.company_id",
    "executiveShortageProduct": "LEFT JOIN commerce_products p ON p.id=entry.value->>'productId' AND p.company_id=s.company_id",
    "chartSettings": "JOIN commerce_settings s ON s.chart_id=a.chart_id"
} as const;
