-- Unified movement history for fulfillment, counts and transfers.
CREATE VIEW inventory_ledger AS
 SELECT m.id,m.warehouse_id,m.product_id,m.quantity_milli,m.value_minor,m.created_at,f.fulfillment_date movement_date,
 'fulfillment' source_type,f.order_id source_id,o.number source_number
 FROM commerce_stock_moves m JOIN commerce_fulfillments f ON f.id=m.fulfillment_id JOIN commerce_orders o ON o.id=f.order_id
 UNION ALL
 SELECT m.id,m.warehouse_id,m.product_id,m.quantity_milli,m.value_minor,m.created_at,m.movement_date,
 'stock_count',m.count_id,c.number FROM inventory_adjustment_moves m JOIN inventory_counts c ON c.id=m.count_id
 UNION ALL
 SELECT m.id,m.warehouse_id,m.product_id,m.quantity_milli,m.value_minor,m.created_at,m.movement_date,
 'transfer',m.transfer_id,t.number FROM inventory_transfer_moves m JOIN inventory_transfers t ON t.id=m.transfer_id;
