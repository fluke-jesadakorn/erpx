import {reservedAvailabilitySumSql} from './read-tail-sql';
/** Owner-native joined facts retain the caller's single SQL snapshot and transaction. */export const productionReadProjection={
  "reservedStock": reservedAvailabilitySumSql('r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id'),
  "quarantinedStock": "inventory_quarantined_quantity(s.warehouse_id,s.product_id)",
  "reservedAvailability": reservedAvailabilitySumSql('WHERE warehouse_id=? AND product_id=?'),
  "genealogyInputLot": "JOIN inventory_lots il ON il.id=i.lot_id",
  "genealogyOutputLot": "JOIN inventory_lots l ON l.id=ol.lot_id"
} as const;
