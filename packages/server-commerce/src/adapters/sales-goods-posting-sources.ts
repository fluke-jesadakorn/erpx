export const salesGoodsPostingFulfillmentSources="SELECT f.id,f.invoice_id,o.company_id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE o.kind='sales'";
