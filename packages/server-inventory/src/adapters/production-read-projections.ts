/** Owner-native joined facts retain the caller's single SQL snapshot and transaction. */export const productionReadProjection={
  "reservedStock": "COALESCE((SELECT sum(quantity_milli) FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0)",
  "quarantinedStock": "inventory_quarantined_quantity(s.warehouse_id,s.product_id)",
  "reservedAvailability": "COALESCE((SELECT sum(quantity_milli) FROM inventory_active_reservations WHERE warehouse_id=? AND product_id=?),0)",
  "genealogyInputLot": "JOIN inventory_lots il ON il.id=i.lot_id",
  "genealogyOutputLot": "JOIN inventory_lots l ON l.id=ol.lot_id"
} as const;
