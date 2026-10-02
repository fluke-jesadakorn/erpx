ALTER TABLE pos_lines ADD COLUMN bill_discount_minor BIGINT NOT NULL DEFAULT 0 CHECK(bill_discount_minor>=0 AND bill_discount_minor<=discount_minor);
-- Historical bills already snapshot line prices and their issuer. Backfill without any postings.
INSERT INTO pos_sale_documents(sale_id,company_id,branch_id,snapshot_json,created_at)
 SELECT s.id,s.company_id,s.branch_id,jsonb_build_object(
  'schema',1,'saleId',s.id,'number',s.number,'currency',s.currency,'businessDate',s.business_date,
  'issuer',s.issuer_json::jsonb,'customer',(SELECT jsonb_build_object('id',p.id,'name',p.name,'phone',p.phone,'notification_email',p.notification_email,'address_json',p.address_json,'tax_registration',p.tax_registration) FROM finance_parties p WHERE p.id=s.party_id),
  'lines',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',l.id,'name',l.name,'quantity_milli',l.quantity_milli,'price_minor',l.price_minor,'tax_bps',l.tax_bps,'tax_inclusive',l.tax_inclusive,'discount_minor',l.discount_minor,'subtotal_minor',l.subtotal_minor,'tax_minor',l.tax_minor,'modifiers_json',l.modifiers_json,'note',l.note) ORDER BY l.id) FROM pos_lines l WHERE l.sale_id=s.id),'[]'),
  'payments',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',p.id,'method',p.method,'amount_minor',p.amount_minor,'tendered_minor',p.tendered_minor,'reference',p.reference,'finance_payment_id',p.finance_payment_id) ORDER BY p.created_at,p.id) FROM pos_payments p WHERE p.sale_id=s.id AND p.state='succeeded'),'[]'),
  'subtotalMinor',s.subtotal_minor,'discountMinor',s.discount_minor,'taxMinor',s.tax_minor,'totalMinor',s.total_minor,
  'roomCharge',(SELECT jsonb_build_object('property_id',ps.property_id,'folio_entry_id',ps.record_id) FROM pms_sources ps WHERE ps.source_key='pos:'||s.id||':'||s.id),
  'historicalBackfill',true),s.updated_at
 FROM pos_sales s WHERE s.state='completed';
