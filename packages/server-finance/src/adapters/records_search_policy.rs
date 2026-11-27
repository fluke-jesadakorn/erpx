//! Finance-owned row predicate for native Records Search.
//!
//! The port is the exact SQL shape of `financeWorkspaceSearchScope` in
//! `packages/server-finance/src/adapters/document-purchases.ts`, expressed
//! against the outer Records alias `workspace_records` and the materialized
//! one-row actor CTE `records_search_actor(assignment_id,user_id,role,now_ms)`
//! joined by Records. This module executes no SQL and owns no binds.
//!
//! Bind and mode contract for the lead:
//! * Composition binds assignment id, user id, role and the sampled clock into
//!   `records_search_actor`. The owner fragment contains no `?`, `$n` or
//!   literal actor value.
//! * `assignmentReadSql('workspace_records','finance.read')` is byte-pinned
//!   with its three `?` placeholders; they are replaced, in order, by the CTE
//!   columns. The IAM active checks keep their original `clock_timestamp()`.
//! * `accounting_history` is the Commercial History decision for the selected
//!   Company at the same clock, supplied by composition. It is applied only in
//!   enforced mode. Shadow mode observes that decision but, like disabled
//!   mode, does not add an ordinary-row purchase gate or assignment gate.
//! * `production` selects the scheduled-rollout variant of the initial-stock
//!   row filter. Both variants are byte-pinned from the authoritative TS
//!   helper; no dev `TRUE` is reused in production.
//! * Every mode keeps the initial-stock marker filter and the assignment gate
//!   on valid initial rows before the Records scan limit.

/// Source-existence predicate for the sales, Manufacturing, POS and Service
/// initial physical-stock Count variants. Generated from the Finance owner
/// helpers (rootOwnerExistsSql, the four *InitialStockJournalPredicate
/// renderers and inventoryFinanceRecordSources) with outer alias
/// `workspace_records`; no clock, bind or mode branch is embedded in it.
const INITIAL_STOCK_SQL: &str = r##"EXISTS(SELECT 1 FROM finance_journals d WHERE d.id=workspace_records.id AND d.company_id=workspace_records.company_id AND (EXISTS(SELECT 1 FROM (SELECT d.id,d.company_id UNION SELECT p.id,p.company_id FROM finance_book_postings b JOIN finance_journals p ON p.id=b.primary_journal_id AND p.company_id=d.company_id WHERE b.secondary_journal_id=d.id) root JOIN (SELECT source.company_id,c.journal_id id FROM (SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'_initialPhysicalStockCount' source_proof
 FROM inventory_counts c JOIN command_receipts receipt ON receipt.operation='inventory.count.create' AND receipt.record_id=c.id AND c.id='count-'||receipt.command_id AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id
 JOIN inventory_count_lines line ON line.count_id=c.id
 JOIN command_receipts goods ON goods.command_id=receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'goodsSetupCommandId' AND goods.operation='commerce.sales-goods-posting-setup.initialize' AND goods.record_id=c.company_id AND goods.authority_json::jsonb->>'companyId'=c.company_id AND goods.result_json::jsonb->>'id'=c.company_id AND goods.result_json::jsonb->>'status'='configured' AND goods.result_json::jsonb->>'version'='1' AND goods.request_hash=receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'goodsSetupRequestHash'
 WHERE jsonb_exists(receipt.result_json::jsonb,'_initialPhysicalStockCount') AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'kind'='initial-physical-stock-count'
 AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'countId'=c.id AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'companyId'=c.company_id
 AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'branchId'=c.branch_id AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'warehouseId'=c.warehouse_id AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'productId'=line.product_id AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'countCommandId'=receipt.command_id
 AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1'
 AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0 AND c.quality_hold_id IS NULL
 AND (SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1) source JOIN inventory_counts c ON c.id=source.id AND c.company_id=source.company_id AND c.status='posted' JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.chart_id=source.source_proof->>'chartId' AND j.currency=source.source_proof->>'currency' AND j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted' AND j.is_opening=0) source ON source.id=root.id AND source.company_id=root.company_id) OR EXISTS(SELECT 1 FROM (SELECT d.id,d.company_id UNION SELECT book.primary_journal_id,d.company_id FROM finance_book_postings book WHERE book.secondary_journal_id=d.id AND book.company_id=d.company_id) root JOIN (SELECT source.company_id,c.journal_id id FROM (SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount' source_proof
 FROM inventory_counts c JOIN command_receipts receipt ON receipt.operation='inventory.count.create' AND receipt.record_id=c.id AND c.id='count-'||receipt.command_id AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id
 JOIN inventory_count_lines line ON line.count_id=c.id
 JOIN (SELECT r.* FROM command_receipts r WHERE
 (r.operation='commerce.manufacturing-posting-setup.initialize' AND r.result_json::jsonb->>'id'=r.record_id AND r.result_json::jsonb->>'status'='configured' AND r.result_json::jsonb->>'version'='1' AND r.authority_json::jsonb->>'companyId'=r.record_id)
 OR (r.operation='commerce.manufacturing-posting-setup.confirm-existing' AND jsonb_exists(r.result_json::jsonb,'_manufacturingExistingPostingSetup')
 AND r.authority_json::jsonb->>'companyId'=r.record_id AND r.result_json::jsonb->>'id'=r.record_id AND r.result_json::jsonb->>'status'='confirmed-existing' AND r.result_json::jsonb->>'version'='1'
 AND r.result_json::jsonb->'_manufacturingExistingPostingSetup'->>'kind'='manufacturing-existing-posting-setup'
 AND r.result_json::jsonb->'_manufacturingExistingPostingSetup'->>'companyId'=r.record_id
 AND r.result_json::jsonb->'_manufacturingExistingPostingSetup'->>'confirmationCommandId'=r.command_id
 AND r.result_json::jsonb->'_manufacturingExistingPostingSetup'->'mapping'->>'company_id'=r.record_id)) goods ON goods.command_id=receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'manufacturingSetupCommandId' AND goods.record_id=c.company_id AND goods.authority_json::jsonb->>'companyId'=c.company_id AND goods.result_json::jsonb->>'id'=c.company_id AND goods.result_json::jsonb->>'version'='1' AND goods.request_hash=receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'manufacturingSetupRequestHash'
 AND (goods.operation='commerce.manufacturing-posting-setup.initialize' OR goods.result_json::jsonb->'_manufacturingExistingPostingSetup'->'mapping'->>'chart_id'=receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'chartId' AND goods.result_json::jsonb->'_manufacturingExistingPostingSetup'->'mapping'->>'inventory_account_id'=receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'inventoryAccountId')
 WHERE NOT jsonb_exists(receipt.result_json::jsonb,'_initialPhysicalStockCount') AND jsonb_exists(receipt.result_json::jsonb,'_manufacturingInitialPhysicalStockCount') AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'kind'='manufacturing-initial-physical-stock-count'
 AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'countId'=c.id AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'companyId'=c.company_id
 AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'branchId'=c.branch_id AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'warehouseId'=c.warehouse_id AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'productId'=line.product_id AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'countCommandId'=receipt.command_id
 AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1'
 AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0 AND c.quality_hold_id IS NULL
 AND (SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1) source JOIN inventory_counts c ON c.id=source.id AND c.company_id=source.company_id AND c.status='posted' JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.chart_id=source.source_proof->>'chartId' AND j.currency=source.source_proof->>'currency' AND j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted' AND j.is_opening=0
 WHERE c.net_value_minor>0 AND c.net_value_minor=(SELECT sum(line.counted_value_minor-line.snapshot_value_minor) FROM inventory_count_lines line WHERE line.count_id=c.id)
 AND (SELECT count(*) FROM finance_journal_lines line WHERE line.journal_id=j.id)=2
 AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'inventoryAccountId' AND line.debit_minor=c.net_value_minor AND line.credit_minor=0)
 AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'varianceAccountId' AND line.credit_minor=c.net_value_minor AND line.debit_minor=0)
 AND EXISTS(SELECT 1 FROM (WITH terminals AS(
 SELECT 'count'::text kind,count.id,count.company_id,count.status,count.version,
 event.reviewed_version+1 terminal_version,command.command_id
 FROM inventory_counts count JOIN inventory_count_decisions event ON event.count_id=count.id
 JOIN command_receipts command ON command.command_id=event.command_id AND command.record_id=count.id AND command.actor_id=event.actor_id
 AND command.operation='inventory.count.'||event.action AND command.authority_json::jsonb->>'companyId'=count.company_id
 AND command.result_json::jsonb->>'status'=count.status AND command.result_json::jsonb->>'version'=(event.reviewed_version+1)::text
 WHERE count.status IN ('posted','cancelled','rejected') AND event.action=CASE count.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END
 UNION ALL SELECT 'landed-cost',cost.id,cost.company_id,cost.status,cost.version,event.reviewed_version+1,command.command_id
 FROM inventory_landed_costs cost JOIN inventory_landed_cost_events event ON event.landed_cost_id=cost.id
 JOIN command_receipts command ON command.command_id=event.command_id AND command.record_id=cost.id AND command.actor_id=event.actor_id
 AND command.operation='inventory.landed_cost.'||event.action AND command.authority_json::jsonb->>'companyId'=cost.company_id
 AND command.result_json::jsonb->>'status'=cost.status AND command.result_json::jsonb->>'version'=(event.reviewed_version+1)::text
 WHERE cost.status IN ('posted','cancelled','rejected') AND event.action=CASE cost.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END
 ) SELECT terminal.* FROM terminals terminal WHERE terminal.terminal_version<=terminal.version AND EXISTS(
 WITH RECURSIVE versions(version) AS(SELECT terminal.terminal_version UNION
 SELECT binding.next_version FROM versions JOIN (SELECT source.company_id,source.target_type,source.target_id,
 source.expected_target_version previous_version,source.expected_target_version+1 next_version,
 command.command_id,to_jsonb(event) event_image,to_jsonb(source) set_image
 FROM attachment_sets source JOIN attachment_events event ON event.company_id=source.company_id AND event.set_id=source.id AND event.action='set.bound'
 JOIN command_receipts command ON command.command_id=source.bound_command_id AND command.record_id=source.target_id AND command.actor_id=event.actor_id
 AND command.authority_json::jsonb->>'companyId'=source.company_id
 WHERE source.state='bound' AND event.details_json::jsonb->>'bindingId'=source.target_id AND
 ((command.operation='attachment.record.bind' AND source.component_kind='record' AND event.details_json::jsonb->>'bindingType'='record_document'
 AND command.result_json::jsonb->>'setId'=source.id AND command.result_json::jsonb->>'version'=(source.expected_target_version+1)::text)
 OR(command.operation='attachment.compatibility.bind' AND event.details_json::jsonb->>'bindingType'='legacy_upload_adapter'
 AND command.result_json::jsonb->>'attachmentSetId'=source.id AND command.result_json::jsonb->>'ownerVersion'=(source.expected_target_version+1)::text))) binding ON binding.previous_version=versions.version
 AND binding.company_id=terminal.company_id AND binding.target_id=terminal.id
 AND (terminal.kind='count' AND binding.target_type IN ('stock_count','inventory_stock_count') OR terminal.kind='landed-cost' AND binding.target_type='landed_cost')
 WHERE binding.next_version<=terminal.version) SELECT 1 FROM versions WHERE version=terminal.version)) terminal WHERE terminal.kind='count' AND terminal.id=c.id AND terminal.company_id=c.company_id AND terminal.status=c.status AND terminal.version=c.version)) source ON source.id=root.id AND source.company_id=root.company_id) OR EXISTS(SELECT 1 FROM (SELECT d.id,d.company_id UNION SELECT book.primary_journal_id,d.company_id FROM finance_book_postings book WHERE book.secondary_journal_id=d.id AND book.company_id=d.company_id) root JOIN (SELECT source.company_id,c.journal_id id FROM (SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'_posInitialPhysicalStockCount' source_proof
 FROM inventory_counts c JOIN command_receipts receipt ON receipt.operation='inventory.count.create' AND receipt.record_id=c.id AND c.id='count-'||receipt.command_id AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id
 JOIN inventory_count_lines line ON line.count_id=c.id
 JOIN command_receipts goods ON goods.command_id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posSetupCommandId' AND goods.record_id=c.company_id AND goods.operation='commerce.pos-posting-setup.initialize' AND goods.authority_json::jsonb->>'companyId'=c.company_id AND goods.result_json::jsonb->>'id'=c.company_id AND goods.result_json::jsonb->>'status'='configured' AND goods.result_json::jsonb->>'version'='1' AND goods.request_hash=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posSetupRequestHash'
 JOIN pos_receipts bootstrap ON bootstrap.id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posBootstrapReceiptId' AND bootstrap.company_id=c.company_id AND bootstrap.request_hash=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posBootstrapRequestHash'
 JOIN pos_audit bootstrap_audit ON bootstrap_audit.id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posBootstrapAuditId' AND bootstrap_audit.company_id=c.company_id AND bootstrap_audit.record_id=c.company_id AND bootstrap_audit.operation='settings.bootstrap' AND bootstrap_audit.branch_id IS NULL AND bootstrap_audit.details_json::jsonb->'result'=bootstrap.result_json::jsonb AND bootstrap.result_json::jsonb->>'id'=c.company_id AND bootstrap.result_json::jsonb->>'warehouseId'=c.warehouse_id
 JOIN pos_receipts approval ON approval.id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posApprovalReceiptId' AND approval.company_id=c.company_id AND approval.request_hash=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posApprovalRequestHash'
 JOIN pos_audit approval_audit ON approval_audit.id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posApprovalAuditId' AND approval_audit.company_id=c.company_id AND approval_audit.record_id=c.company_id AND approval_audit.operation='accounting.approve' AND approval_audit.branch_id IS NULL AND approval_audit.actor_id<>bootstrap_audit.actor_id AND approval_audit.details_json::jsonb->'result'=approval.result_json::jsonb AND approval.result_json::jsonb->>'id'=c.company_id AND approval.result_json::jsonb->>'state'='approved'
 WHERE to_regclass('inventory_pos_initial_physical_sources') IS NOT NULL AND jsonb_exists(receipt.result_json::jsonb,'_posInitialPhysicalStockCount') AND NOT jsonb_exists(receipt.result_json::jsonb,'_initialPhysicalStockCount') AND NOT jsonb_exists(receipt.result_json::jsonb,'_manufacturingInitialPhysicalStockCount')
 AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'kind'='pos-initial-physical-stock-count' AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'countId'=c.id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'companyId'=c.company_id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'branchId'=c.branch_id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'warehouseId'=c.warehouse_id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'productId'=line.product_id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'countCommandId'=receipt.command_id
 AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1' AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0 AND c.quality_hold_id IS NULL AND (SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1) source JOIN inventory_counts c ON c.id=source.id AND c.company_id=source.company_id AND c.status='posted' JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.chart_id=source.source_proof->>'chartId' AND j.currency=source.source_proof->>'currency' AND j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted' AND j.is_opening=0
 WHERE c.net_value_minor>0 AND c.net_value_minor=(SELECT sum(line.counted_value_minor-line.snapshot_value_minor) FROM inventory_count_lines line WHERE line.count_id=c.id)
 AND (SELECT count(*) FROM finance_journal_lines line WHERE line.journal_id=j.id)=2
 AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'inventoryAccountId' AND line.debit_minor=c.net_value_minor AND line.credit_minor=0)
 AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'varianceAccountId' AND line.credit_minor=c.net_value_minor AND line.debit_minor=0)
 AND EXISTS(SELECT 1 FROM (WITH terminals AS(
 SELECT 'count'::text kind,count.id,count.company_id,count.status,count.version,
 event.reviewed_version+1 terminal_version,command.command_id
 FROM inventory_counts count JOIN inventory_count_decisions event ON event.count_id=count.id
 JOIN command_receipts command ON command.command_id=event.command_id AND command.record_id=count.id AND command.actor_id=event.actor_id
 AND command.operation='inventory.count.'||event.action AND command.authority_json::jsonb->>'companyId'=count.company_id
 AND command.result_json::jsonb->>'status'=count.status AND command.result_json::jsonb->>'version'=(event.reviewed_version+1)::text
 WHERE count.status IN ('posted','cancelled','rejected') AND event.action=CASE count.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END
 UNION ALL SELECT 'landed-cost',cost.id,cost.company_id,cost.status,cost.version,event.reviewed_version+1,command.command_id
 FROM inventory_landed_costs cost JOIN inventory_landed_cost_events event ON event.landed_cost_id=cost.id
 JOIN command_receipts command ON command.command_id=event.command_id AND command.record_id=cost.id AND command.actor_id=event.actor_id
 AND command.operation='inventory.landed_cost.'||event.action AND command.authority_json::jsonb->>'companyId'=cost.company_id
 AND command.result_json::jsonb->>'status'=cost.status AND command.result_json::jsonb->>'version'=(event.reviewed_version+1)::text
 WHERE cost.status IN ('posted','cancelled','rejected') AND event.action=CASE cost.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END
 ) SELECT terminal.* FROM terminals terminal WHERE terminal.terminal_version<=terminal.version AND EXISTS(
 WITH RECURSIVE versions(version) AS(SELECT terminal.terminal_version UNION
 SELECT binding.next_version FROM versions JOIN (SELECT source.company_id,source.target_type,source.target_id,
 source.expected_target_version previous_version,source.expected_target_version+1 next_version,
 command.command_id,to_jsonb(event) event_image,to_jsonb(source) set_image
 FROM attachment_sets source JOIN attachment_events event ON event.company_id=source.company_id AND event.set_id=source.id AND event.action='set.bound'
 JOIN command_receipts command ON command.command_id=source.bound_command_id AND command.record_id=source.target_id AND command.actor_id=event.actor_id
 AND command.authority_json::jsonb->>'companyId'=source.company_id
 WHERE source.state='bound' AND event.details_json::jsonb->>'bindingId'=source.target_id AND
 ((command.operation='attachment.record.bind' AND source.component_kind='record' AND event.details_json::jsonb->>'bindingType'='record_document'
 AND command.result_json::jsonb->>'setId'=source.id AND command.result_json::jsonb->>'version'=(source.expected_target_version+1)::text)
 OR(command.operation='attachment.compatibility.bind' AND event.details_json::jsonb->>'bindingType'='legacy_upload_adapter'
 AND command.result_json::jsonb->>'attachmentSetId'=source.id AND command.result_json::jsonb->>'ownerVersion'=(source.expected_target_version+1)::text))) binding ON binding.previous_version=versions.version
 AND binding.company_id=terminal.company_id AND binding.target_id=terminal.id
 AND (terminal.kind='count' AND binding.target_type IN ('stock_count','inventory_stock_count') OR terminal.kind='landed-cost' AND binding.target_type='landed_cost')
 WHERE binding.next_version<=terminal.version) SELECT 1 FROM versions WHERE version=terminal.version)) terminal WHERE terminal.kind='count' AND terminal.id=c.id AND terminal.company_id=c.company_id AND terminal.status=c.status AND terminal.version=c.version)) source ON source.id=root.id AND source.company_id=root.company_id) OR EXISTS(SELECT 1 FROM (SELECT d.id,d.company_id UNION SELECT book.primary_journal_id,d.company_id FROM finance_book_postings book WHERE book.secondary_journal_id=d.id AND book.company_id=d.company_id) root JOIN (SELECT source.company_id,c.journal_id id FROM (SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount' source_proof
 FROM inventory_counts c JOIN command_receipts receipt ON receipt.operation='inventory.count.create' AND receipt.record_id=c.id AND c.id='count-'||receipt.command_id AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id
 JOIN inventory_count_lines line ON line.count_id=c.id
 JOIN (SELECT receipt.command_id,receipt.record_id company_id,receipt.request_hash,receipt.result_json::jsonb->'_servicePartsPostingSetup' source_proof,receipt.result_json::jsonb->'_servicePartsPostingSetup'->'mapping' mapping
 FROM command_receipts receipt WHERE to_regclass('commerce_service_parts_posting_sources') IS NOT NULL
 AND receipt.operation IN ('commerce.service-parts-posting-setup.initialize','commerce.service-parts-posting-setup.confirm-existing')
 AND receipt.authority_json::jsonb->>'companyId'=receipt.record_id AND receipt.result_json::jsonb->>'id'=receipt.record_id AND receipt.result_json::jsonb->>'version'='1'
 AND receipt.result_json::jsonb->>'status'=CASE receipt.operation WHEN 'commerce.service-parts-posting-setup.initialize' THEN 'configured' ELSE 'confirmed-existing' END
 AND receipt.result_json::jsonb->'_servicePartsPostingSetup'->>'kind'='service-parts-posting-setup' AND receipt.result_json::jsonb->'_servicePartsPostingSetup'->>'companyId'=receipt.record_id AND receipt.result_json::jsonb->'_servicePartsPostingSetup'->>'commandId'=receipt.command_id AND receipt.result_json::jsonb->'_servicePartsPostingSetup'->'mapping'->>'company_id'=receipt.record_id) goods ON goods.command_id=receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'serviceSetupCommandId' AND goods.company_id=c.company_id AND goods.request_hash=receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'serviceSetupRequestHash'
 AND goods.mapping->>'chart_id'=receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'chartId' AND goods.mapping->>'inventory_account_id'=receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'inventoryAccountId' AND goods.mapping->>'currency'=receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'currency'
 WHERE to_regclass('inventory_service_initial_physical_sources') IS NOT NULL AND NOT jsonb_exists(receipt.result_json::jsonb,'_initialPhysicalStockCount') AND NOT jsonb_exists(receipt.result_json::jsonb,'_manufacturingInitialPhysicalStockCount') AND NOT jsonb_exists(receipt.result_json::jsonb,'_posInitialPhysicalStockCount') AND jsonb_exists(receipt.result_json::jsonb,'_serviceInitialPhysicalStockCount') AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'kind'='service-initial-physical-stock-count'
 AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'countId'=c.id AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'companyId'=c.company_id
 AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'branchId'=c.branch_id AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'warehouseId'=c.warehouse_id AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'productId'=line.product_id AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'countCommandId'=receipt.command_id
 AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1'
 AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0 AND c.quality_hold_id IS NULL
 AND (SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1) source JOIN inventory_counts c ON c.id=source.id AND c.company_id=source.company_id AND c.status='posted' JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.chart_id=source.source_proof->>'chartId' AND j.currency=source.source_proof->>'currency' AND j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted' AND j.is_opening=0
 WHERE c.net_value_minor>0 AND c.net_value_minor=(SELECT sum(line.counted_value_minor-line.snapshot_value_minor) FROM inventory_count_lines line WHERE line.count_id=c.id)
 AND (SELECT count(*) FROM finance_journal_lines line WHERE line.journal_id=j.id)=2
 AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'inventoryAccountId' AND line.debit_minor=c.net_value_minor AND line.credit_minor=0)
 AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'varianceAccountId' AND line.credit_minor=c.net_value_minor AND line.debit_minor=0)
 AND EXISTS(SELECT 1 FROM (WITH terminals AS(
 SELECT 'count'::text kind,count.id,count.company_id,count.status,count.version,
 event.reviewed_version+1 terminal_version,command.command_id
 FROM inventory_counts count JOIN inventory_count_decisions event ON event.count_id=count.id
 JOIN command_receipts command ON command.command_id=event.command_id AND command.record_id=count.id AND command.actor_id=event.actor_id
 AND command.operation='inventory.count.'||event.action AND command.authority_json::jsonb->>'companyId'=count.company_id
 AND command.result_json::jsonb->>'status'=count.status AND command.result_json::jsonb->>'version'=(event.reviewed_version+1)::text
 WHERE count.status IN ('posted','cancelled','rejected') AND event.action=CASE count.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END
 UNION ALL SELECT 'landed-cost',cost.id,cost.company_id,cost.status,cost.version,event.reviewed_version+1,command.command_id
 FROM inventory_landed_costs cost JOIN inventory_landed_cost_events event ON event.landed_cost_id=cost.id
 JOIN command_receipts command ON command.command_id=event.command_id AND command.record_id=cost.id AND command.actor_id=event.actor_id
 AND command.operation='inventory.landed_cost.'||event.action AND command.authority_json::jsonb->>'companyId'=cost.company_id
 AND command.result_json::jsonb->>'status'=cost.status AND command.result_json::jsonb->>'version'=(event.reviewed_version+1)::text
 WHERE cost.status IN ('posted','cancelled','rejected') AND event.action=CASE cost.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END
 ) SELECT terminal.* FROM terminals terminal WHERE terminal.terminal_version<=terminal.version AND EXISTS(
 WITH RECURSIVE versions(version) AS(SELECT terminal.terminal_version UNION
 SELECT binding.next_version FROM versions JOIN (SELECT source.company_id,source.target_type,source.target_id,
 source.expected_target_version previous_version,source.expected_target_version+1 next_version,
 command.command_id,to_jsonb(event) event_image,to_jsonb(source) set_image
 FROM attachment_sets source JOIN attachment_events event ON event.company_id=source.company_id AND event.set_id=source.id AND event.action='set.bound'
 JOIN command_receipts command ON command.command_id=source.bound_command_id AND command.record_id=source.target_id AND command.actor_id=event.actor_id
 AND command.authority_json::jsonb->>'companyId'=source.company_id
 WHERE source.state='bound' AND event.details_json::jsonb->>'bindingId'=source.target_id AND
 ((command.operation='attachment.record.bind' AND source.component_kind='record' AND event.details_json::jsonb->>'bindingType'='record_document'
 AND command.result_json::jsonb->>'setId'=source.id AND command.result_json::jsonb->>'version'=(source.expected_target_version+1)::text)
 OR(command.operation='attachment.compatibility.bind' AND event.details_json::jsonb->>'bindingType'='legacy_upload_adapter'
 AND command.result_json::jsonb->>'attachmentSetId'=source.id AND command.result_json::jsonb->>'ownerVersion'=(source.expected_target_version+1)::text))) binding ON binding.previous_version=versions.version
 AND binding.company_id=terminal.company_id AND binding.target_id=terminal.id
 AND (terminal.kind='count' AND binding.target_type IN ('stock_count','inventory_stock_count') OR terminal.kind='landed-cost' AND binding.target_type='landed_cost')
 WHERE binding.next_version<=terminal.version) SELECT 1 FROM versions WHERE version=terminal.version)) terminal WHERE terminal.kind='count' AND terminal.id=c.id AND terminal.company_id=c.company_id AND terminal.status=c.status AND terminal.version=c.version)) source ON source.id=root.id AND source.company_id=root.company_id)))"##;

/// The exact `assignmentReadSql('workspace_records','finance.read')` text with
/// its three TS positional binds. Substituting the CTE columns is the only
/// adaptation: nothing else in this fragment is rewritten.
const ASSIGNMENT_TS_SQL: &str = r##"EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=workspace_records.company_id
    AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_role_allows(g.role,g.company_id,'finance.read')
    AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=workspace_records.branch_id OR g.scope_kind='department' AND g.scope_node_id=workspace_records.department_id))"##;

/// Initial-stock row filter generated with `NODE_ENV=production`, where the
/// scheduled feature predicates are real rollout/release/engineering guards
/// rather than the dev-only `TRUE`.
const ROW_FILTER_PRODUCTION_SQL: &str = r##"(NOT (EXISTS(SELECT 1 FROM finance_journals initial_finance_document WHERE initial_finance_document.id=workspace_records.id AND initial_finance_document.company_id=workspace_records.company_id AND (EXISTS(SELECT 1 FROM (SELECT initial_finance_document.id,initial_finance_document.company_id UNION SELECT p.id,p.company_id FROM finance_book_postings b JOIN finance_journals p ON p.id=b.primary_journal_id AND p.company_id=initial_finance_document.company_id WHERE b.secondary_journal_id=initial_finance_document.id) root JOIN (SELECT c.company_id,c.journal_id id FROM inventory_counts c JOIN (SELECT record_id id,authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'_initialPhysicalStockCount')) marker ON marker.id=c.id WHERE c.journal_id IS NOT NULL) marker ON marker.id=root.id AND marker.company_id=root.company_id) OR EXISTS(SELECT 1 FROM (SELECT initial_finance_document.id,initial_finance_document.company_id UNION SELECT book.primary_journal_id,initial_finance_document.company_id FROM finance_book_postings book WHERE book.secondary_journal_id=initial_finance_document.id AND book.company_id=initial_finance_document.company_id) root JOIN (SELECT c.company_id,c.journal_id id FROM inventory_counts c JOIN (SELECT record_id id,authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'_manufacturingInitialPhysicalStockCount')) marker ON marker.id=c.id WHERE c.journal_id IS NOT NULL) marker ON marker.id=root.id AND marker.company_id=root.company_id) OR EXISTS(SELECT 1 FROM (SELECT initial_finance_document.id,initial_finance_document.company_id UNION SELECT book.primary_journal_id,initial_finance_document.company_id FROM finance_book_postings book WHERE book.secondary_journal_id=initial_finance_document.id AND book.company_id=initial_finance_document.company_id) root JOIN (SELECT c.company_id,c.journal_id id FROM inventory_counts c JOIN (SELECT record_id id,authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'_posInitialPhysicalStockCount')) marker ON marker.id=c.id WHERE c.journal_id IS NOT NULL) marker ON marker.id=root.id AND marker.company_id=root.company_id) OR EXISTS(SELECT 1 FROM (SELECT initial_finance_document.id,initial_finance_document.company_id UNION SELECT book.primary_journal_id,initial_finance_document.company_id FROM finance_book_postings book WHERE book.secondary_journal_id=initial_finance_document.id AND book.company_id=initial_finance_document.company_id) root JOIN (SELECT c.company_id,c.journal_id id FROM inventory_counts c JOIN (SELECT record_id id,authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'_serviceInitialPhysicalStockCount')) marker ON marker.id=c.id WHERE c.journal_id IS NOT NULL) marker ON marker.id=root.id AND marker.company_id=root.company_id)))) OR (TRUE AND (CASE WHEN EXISTS(SELECT 1 FROM (SELECT workspace_records.id,workspace_records.company_id UNION SELECT book.primary_journal_id,workspace_records.company_id FROM finance_book_postings book WHERE book.secondary_journal_id=workspace_records.id AND book.company_id=workspace_records.company_id) root JOIN (SELECT c.company_id,c.journal_id id FROM inventory_counts c JOIN (SELECT record_id id,authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'_serviceInitialPhysicalStockCount')) marker ON marker.id=c.id WHERE c.journal_id IS NOT NULL) marker ON marker.id=root.id AND marker.company_id=root.company_id) THEN (EXISTS(SELECT 1 FROM (SELECT workspace_records.id,workspace_records.company_id UNION SELECT book.primary_journal_id,workspace_records.company_id FROM finance_book_postings book WHERE book.secondary_journal_id=workspace_records.id AND book.company_id=workspace_records.company_id) root JOIN (SELECT source.company_id,c.journal_id id FROM (SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount' source_proof
 FROM inventory_counts c JOIN command_receipts receipt ON receipt.operation='inventory.count.create' AND receipt.record_id=c.id AND c.id='count-'||receipt.command_id AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id
 JOIN inventory_count_lines line ON line.count_id=c.id
 JOIN (SELECT receipt.command_id,receipt.record_id company_id,receipt.request_hash,receipt.result_json::jsonb->'_servicePartsPostingSetup' source_proof,receipt.result_json::jsonb->'_servicePartsPostingSetup'->'mapping' mapping
 FROM command_receipts receipt WHERE to_regclass('commerce_service_parts_posting_sources') IS NOT NULL
 AND receipt.operation IN ('commerce.service-parts-posting-setup.initialize','commerce.service-parts-posting-setup.confirm-existing')
 AND receipt.authority_json::jsonb->>'companyId'=receipt.record_id AND receipt.result_json::jsonb->>'id'=receipt.record_id AND receipt.result_json::jsonb->>'version'='1'
 AND receipt.result_json::jsonb->>'status'=CASE receipt.operation WHEN 'commerce.service-parts-posting-setup.initialize' THEN 'configured' ELSE 'confirmed-existing' END
 AND receipt.result_json::jsonb->'_servicePartsPostingSetup'->>'kind'='service-parts-posting-setup' AND receipt.result_json::jsonb->'_servicePartsPostingSetup'->>'companyId'=receipt.record_id AND receipt.result_json::jsonb->'_servicePartsPostingSetup'->>'commandId'=receipt.command_id AND receipt.result_json::jsonb->'_servicePartsPostingSetup'->'mapping'->>'company_id'=receipt.record_id) goods ON goods.command_id=receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'serviceSetupCommandId' AND goods.company_id=c.company_id AND goods.request_hash=receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'serviceSetupRequestHash'
 AND goods.mapping->>'chart_id'=receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'chartId' AND goods.mapping->>'inventory_account_id'=receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'inventoryAccountId' AND goods.mapping->>'currency'=receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'currency'
 WHERE to_regclass('inventory_service_initial_physical_sources') IS NOT NULL AND NOT jsonb_exists(receipt.result_json::jsonb,'_initialPhysicalStockCount') AND NOT jsonb_exists(receipt.result_json::jsonb,'_manufacturingInitialPhysicalStockCount') AND NOT jsonb_exists(receipt.result_json::jsonb,'_posInitialPhysicalStockCount') AND jsonb_exists(receipt.result_json::jsonb,'_serviceInitialPhysicalStockCount') AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'kind'='service-initial-physical-stock-count'
 AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'countId'=c.id AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'companyId'=c.company_id
 AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'branchId'=c.branch_id AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'warehouseId'=c.warehouse_id AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'productId'=line.product_id AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'->>'countCommandId'=receipt.command_id
 AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1'
 AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0 AND c.quality_hold_id IS NULL
 AND (SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1) source JOIN inventory_counts c ON c.id=source.id AND c.company_id=source.company_id AND c.status='posted' JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.chart_id=source.source_proof->>'chartId' AND j.currency=source.source_proof->>'currency' AND j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted' AND j.is_opening=0
 WHERE c.net_value_minor>0 AND c.net_value_minor=(SELECT sum(line.counted_value_minor-line.snapshot_value_minor) FROM inventory_count_lines line WHERE line.count_id=c.id)
 AND (SELECT count(*) FROM finance_journal_lines line WHERE line.journal_id=j.id)=2
 AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'inventoryAccountId' AND line.debit_minor=c.net_value_minor AND line.credit_minor=0)
 AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'varianceAccountId' AND line.credit_minor=c.net_value_minor AND line.debit_minor=0)
 AND EXISTS(SELECT 1 FROM (WITH terminals AS(
 SELECT 'count'::text kind,count.id,count.company_id,count.status,count.version,
 event.reviewed_version+1 terminal_version,command.command_id
 FROM inventory_counts count JOIN inventory_count_decisions event ON event.count_id=count.id
 JOIN command_receipts command ON command.command_id=event.command_id AND command.record_id=count.id AND command.actor_id=event.actor_id
 AND command.operation='inventory.count.'||event.action AND command.authority_json::jsonb->>'companyId'=count.company_id
 AND command.result_json::jsonb->>'status'=count.status AND command.result_json::jsonb->>'version'=(event.reviewed_version+1)::text
 WHERE count.status IN ('posted','cancelled','rejected') AND event.action=CASE count.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END
 UNION ALL SELECT 'landed-cost',cost.id,cost.company_id,cost.status,cost.version,event.reviewed_version+1,command.command_id
 FROM inventory_landed_costs cost JOIN inventory_landed_cost_events event ON event.landed_cost_id=cost.id
 JOIN command_receipts command ON command.command_id=event.command_id AND command.record_id=cost.id AND command.actor_id=event.actor_id
 AND command.operation='inventory.landed_cost.'||event.action AND command.authority_json::jsonb->>'companyId'=cost.company_id
 AND command.result_json::jsonb->>'status'=cost.status AND command.result_json::jsonb->>'version'=(event.reviewed_version+1)::text
 WHERE cost.status IN ('posted','cancelled','rejected') AND event.action=CASE cost.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END
 ) SELECT terminal.* FROM terminals terminal WHERE terminal.terminal_version<=terminal.version AND EXISTS(
 WITH RECURSIVE versions(version) AS(SELECT terminal.terminal_version UNION
 SELECT binding.next_version FROM versions JOIN (SELECT source.company_id,source.target_type,source.target_id,
 source.expected_target_version previous_version,source.expected_target_version+1 next_version,
 command.command_id,to_jsonb(event) event_image,to_jsonb(source) set_image
 FROM attachment_sets source JOIN attachment_events event ON event.company_id=source.company_id AND event.set_id=source.id AND event.action='set.bound'
 JOIN command_receipts command ON command.command_id=source.bound_command_id AND command.record_id=source.target_id AND command.actor_id=event.actor_id
 AND command.authority_json::jsonb->>'companyId'=source.company_id
 WHERE source.state='bound' AND event.details_json::jsonb->>'bindingId'=source.target_id AND
 ((command.operation='attachment.record.bind' AND source.component_kind='record' AND event.details_json::jsonb->>'bindingType'='record_document'
 AND command.result_json::jsonb->>'setId'=source.id AND command.result_json::jsonb->>'version'=(source.expected_target_version+1)::text)
 OR(command.operation='attachment.compatibility.bind' AND event.details_json::jsonb->>'bindingType'='legacy_upload_adapter'
 AND command.result_json::jsonb->>'attachmentSetId'=source.id AND command.result_json::jsonb->>'ownerVersion'=(source.expected_target_version+1)::text))) binding ON binding.previous_version=versions.version
 AND binding.company_id=terminal.company_id AND binding.target_id=terminal.id
 AND (terminal.kind='count' AND binding.target_type IN ('stock_count','inventory_stock_count') OR terminal.kind='landed-cost' AND binding.target_type='landed_cost')
 WHERE binding.next_version<=terminal.version) SELECT 1 FROM versions WHERE version=terminal.version)) terminal WHERE terminal.kind='count' AND terminal.id=c.id AND terminal.company_id=c.company_id AND terminal.status=c.status AND terminal.version=c.version)) source ON source.id=root.id AND source.company_id=root.company_id) AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=workspace_records.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=workspace_records.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='service' AND purchase.package_version=1))) AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=workspace_records.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=workspace_records.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='service-parts-assets' AND purchase.package_version=1) AND EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=workspace_records.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=workspace_records.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') parent WHERE parent.package_id='service' AND parent.package_version IN (1)))) AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=workspace_records.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=workspace_records.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='service' AND purchase.package_version=1))) AND FALSE AND EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=workspace_records.company_id AND f.module_id='inventory' AND (CASE WHEN f.module_id='legal' THEN f.rollout_state IN ('pilot','available') AND FALSE ELSE (f.rollout_state='pilot' OR (f.rollout_state='available' AND FALSE)) END)) AND EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=workspace_records.company_id AND f.module_id='finance' AND (CASE WHEN f.module_id='legal' THEN f.rollout_state IN ('pilot','available') AND FALSE ELSE (f.rollout_state='pilot' OR (f.rollout_state='available' AND FALSE)) END))) WHEN EXISTS(SELECT 1 FROM (SELECT workspace_records.id,workspace_records.company_id UNION SELECT book.primary_journal_id,workspace_records.company_id FROM finance_book_postings book WHERE book.secondary_journal_id=workspace_records.id AND book.company_id=workspace_records.company_id) root JOIN (SELECT c.company_id,c.journal_id id FROM inventory_counts c JOIN (SELECT record_id id,authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'_posInitialPhysicalStockCount')) marker ON marker.id=c.id WHERE c.journal_id IS NOT NULL) marker ON marker.id=root.id AND marker.company_id=root.company_id) THEN (EXISTS(SELECT 1 FROM (SELECT workspace_records.id,workspace_records.company_id UNION SELECT book.primary_journal_id,workspace_records.company_id FROM finance_book_postings book WHERE book.secondary_journal_id=workspace_records.id AND book.company_id=workspace_records.company_id) root JOIN (SELECT source.company_id,c.journal_id id FROM (SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'_posInitialPhysicalStockCount' source_proof
 FROM inventory_counts c JOIN command_receipts receipt ON receipt.operation='inventory.count.create' AND receipt.record_id=c.id AND c.id='count-'||receipt.command_id AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id
 JOIN inventory_count_lines line ON line.count_id=c.id
 JOIN command_receipts goods ON goods.command_id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posSetupCommandId' AND goods.record_id=c.company_id AND goods.operation='commerce.pos-posting-setup.initialize' AND goods.authority_json::jsonb->>'companyId'=c.company_id AND goods.result_json::jsonb->>'id'=c.company_id AND goods.result_json::jsonb->>'status'='configured' AND goods.result_json::jsonb->>'version'='1' AND goods.request_hash=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posSetupRequestHash'
 JOIN pos_receipts bootstrap ON bootstrap.id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posBootstrapReceiptId' AND bootstrap.company_id=c.company_id AND bootstrap.request_hash=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posBootstrapRequestHash'
 JOIN pos_audit bootstrap_audit ON bootstrap_audit.id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posBootstrapAuditId' AND bootstrap_audit.company_id=c.company_id AND bootstrap_audit.record_id=c.company_id AND bootstrap_audit.operation='settings.bootstrap' AND bootstrap_audit.branch_id IS NULL AND bootstrap_audit.details_json::jsonb->'result'=bootstrap.result_json::jsonb AND bootstrap.result_json::jsonb->>'id'=c.company_id AND bootstrap.result_json::jsonb->>'warehouseId'=c.warehouse_id
 JOIN pos_receipts approval ON approval.id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posApprovalReceiptId' AND approval.company_id=c.company_id AND approval.request_hash=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posApprovalRequestHash'
 JOIN pos_audit approval_audit ON approval_audit.id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posApprovalAuditId' AND approval_audit.company_id=c.company_id AND approval_audit.record_id=c.company_id AND approval_audit.operation='accounting.approve' AND approval_audit.branch_id IS NULL AND approval_audit.actor_id<>bootstrap_audit.actor_id AND approval_audit.details_json::jsonb->'result'=approval.result_json::jsonb AND approval.result_json::jsonb->>'id'=c.company_id AND approval.result_json::jsonb->>'state'='approved'
 WHERE to_regclass('inventory_pos_initial_physical_sources') IS NOT NULL AND jsonb_exists(receipt.result_json::jsonb,'_posInitialPhysicalStockCount') AND NOT jsonb_exists(receipt.result_json::jsonb,'_initialPhysicalStockCount') AND NOT jsonb_exists(receipt.result_json::jsonb,'_manufacturingInitialPhysicalStockCount')
 AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'kind'='pos-initial-physical-stock-count' AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'countId'=c.id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'companyId'=c.company_id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'branchId'=c.branch_id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'warehouseId'=c.warehouse_id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'productId'=line.product_id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'countCommandId'=receipt.command_id
 AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1' AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0 AND c.quality_hold_id IS NULL AND (SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1) source JOIN inventory_counts c ON c.id=source.id AND c.company_id=source.company_id AND c.status='posted' JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.chart_id=source.source_proof->>'chartId' AND j.currency=source.source_proof->>'currency' AND j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted' AND j.is_opening=0
 WHERE c.net_value_minor>0 AND c.net_value_minor=(SELECT sum(line.counted_value_minor-line.snapshot_value_minor) FROM inventory_count_lines line WHERE line.count_id=c.id)
 AND (SELECT count(*) FROM finance_journal_lines line WHERE line.journal_id=j.id)=2
 AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'inventoryAccountId' AND line.debit_minor=c.net_value_minor AND line.credit_minor=0)
 AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'varianceAccountId' AND line.credit_minor=c.net_value_minor AND line.debit_minor=0)
 AND EXISTS(SELECT 1 FROM (WITH terminals AS(
 SELECT 'count'::text kind,count.id,count.company_id,count.status,count.version,
 event.reviewed_version+1 terminal_version,command.command_id
 FROM inventory_counts count JOIN inventory_count_decisions event ON event.count_id=count.id
 JOIN command_receipts command ON command.command_id=event.command_id AND command.record_id=count.id AND command.actor_id=event.actor_id
 AND command.operation='inventory.count.'||event.action AND command.authority_json::jsonb->>'companyId'=count.company_id
 AND command.result_json::jsonb->>'status'=count.status AND command.result_json::jsonb->>'version'=(event.reviewed_version+1)::text
 WHERE count.status IN ('posted','cancelled','rejected') AND event.action=CASE count.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END
 UNION ALL SELECT 'landed-cost',cost.id,cost.company_id,cost.status,cost.version,event.reviewed_version+1,command.command_id
 FROM inventory_landed_costs cost JOIN inventory_landed_cost_events event ON event.landed_cost_id=cost.id
 JOIN command_receipts command ON command.command_id=event.command_id AND command.record_id=cost.id AND command.actor_id=event.actor_id
 AND command.operation='inventory.landed_cost.'||event.action AND command.authority_json::jsonb->>'companyId'=cost.company_id
 AND command.result_json::jsonb->>'status'=cost.status AND command.result_json::jsonb->>'version'=(event.reviewed_version+1)::text
 WHERE cost.status IN ('posted','cancelled','rejected') AND event.action=CASE cost.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END
 ) SELECT terminal.* FROM terminals terminal WHERE terminal.terminal_version<=terminal.version AND EXISTS(
 WITH RECURSIVE versions(version) AS(SELECT terminal.terminal_version UNION
 SELECT binding.next_version FROM versions JOIN (SELECT source.company_id,source.target_type,source.target_id,
 source.expected_target_version previous_version,source.expected_target_version+1 next_version,
 command.command_id,to_jsonb(event) event_image,to_jsonb(source) set_image
 FROM attachment_sets source JOIN attachment_events event ON event.company_id=source.company_id AND event.set_id=source.id AND event.action='set.bound'
 JOIN command_receipts command ON command.command_id=source.bound_command_id AND command.record_id=source.target_id AND command.actor_id=event.actor_id
 AND command.authority_json::jsonb->>'companyId'=source.company_id
 WHERE source.state='bound' AND event.details_json::jsonb->>'bindingId'=source.target_id AND
 ((command.operation='attachment.record.bind' AND source.component_kind='record' AND event.details_json::jsonb->>'bindingType'='record_document'
 AND command.result_json::jsonb->>'setId'=source.id AND command.result_json::jsonb->>'version'=(source.expected_target_version+1)::text)
 OR(command.operation='attachment.compatibility.bind' AND event.details_json::jsonb->>'bindingType'='legacy_upload_adapter'
 AND command.result_json::jsonb->>'attachmentSetId'=source.id AND command.result_json::jsonb->>'ownerVersion'=(source.expected_target_version+1)::text))) binding ON binding.previous_version=versions.version
 AND binding.company_id=terminal.company_id AND binding.target_id=terminal.id
 AND (terminal.kind='count' AND binding.target_type IN ('stock_count','inventory_stock_count') OR terminal.kind='landed-cost' AND binding.target_type='landed_cost')
 WHERE binding.next_version<=terminal.version) SELECT 1 FROM versions WHERE version=terminal.version)) terminal WHERE terminal.kind='count' AND terminal.id=c.id AND terminal.company_id=c.company_id AND terminal.status=c.status AND terminal.version=c.version)) source ON source.id=root.id AND source.company_id=root.company_id) AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=workspace_records.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=workspace_records.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='pos' AND purchase.package_version=1))) AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=workspace_records.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=workspace_records.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='pos' AND purchase.package_version=1)) OR (EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=workspace_records.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=workspace_records.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='pms' AND purchase.package_version=1)) OR (EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=workspace_records.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=workspace_records.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='sales' AND purchase.package_version=1)) OR (EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=workspace_records.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=workspace_records.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='manufacturing' AND purchase.package_version=1))) AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=workspace_records.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=workspace_records.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='pos' AND purchase.package_version=1))) AND EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=workspace_records.company_id AND f.module_id='pos' AND (CASE WHEN f.module_id='legal' THEN f.rollout_state IN ('pilot','available') AND FALSE ELSE (f.rollout_state='pilot' OR (f.rollout_state='available' AND FALSE)) END)) AND EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=workspace_records.company_id AND f.module_id='inventory' AND (CASE WHEN f.module_id='legal' THEN f.rollout_state IN ('pilot','available') AND FALSE ELSE (f.rollout_state='pilot' OR (f.rollout_state='available' AND FALSE)) END)) AND EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=workspace_records.company_id AND f.module_id='finance' AND (CASE WHEN f.module_id='legal' THEN f.rollout_state IN ('pilot','available') AND FALSE ELSE (f.rollout_state='pilot' OR (f.rollout_state='available' AND FALSE)) END))) WHEN EXISTS(SELECT 1 FROM (SELECT workspace_records.id,workspace_records.company_id UNION SELECT book.primary_journal_id,workspace_records.company_id FROM finance_book_postings book WHERE book.secondary_journal_id=workspace_records.id AND book.company_id=workspace_records.company_id) root JOIN (SELECT c.company_id,c.journal_id id FROM inventory_counts c JOIN (SELECT record_id id,authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'_manufacturingInitialPhysicalStockCount')) marker ON marker.id=c.id WHERE c.journal_id IS NOT NULL) marker ON marker.id=root.id AND marker.company_id=root.company_id) THEN (EXISTS(SELECT 1 FROM (SELECT workspace_records.id,workspace_records.company_id UNION SELECT book.primary_journal_id,workspace_records.company_id FROM finance_book_postings book WHERE book.secondary_journal_id=workspace_records.id AND book.company_id=workspace_records.company_id) root JOIN (SELECT source.company_id,c.journal_id id FROM (SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount' source_proof
 FROM inventory_counts c JOIN command_receipts receipt ON receipt.operation='inventory.count.create' AND receipt.record_id=c.id AND c.id='count-'||receipt.command_id AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id
 JOIN inventory_count_lines line ON line.count_id=c.id
 JOIN (SELECT r.* FROM command_receipts r WHERE
 (r.operation='commerce.manufacturing-posting-setup.initialize' AND r.result_json::jsonb->>'id'=r.record_id AND r.result_json::jsonb->>'status'='configured' AND r.result_json::jsonb->>'version'='1' AND r.authority_json::jsonb->>'companyId'=r.record_id)
 OR (r.operation='commerce.manufacturing-posting-setup.confirm-existing' AND jsonb_exists(r.result_json::jsonb,'_manufacturingExistingPostingSetup')
 AND r.authority_json::jsonb->>'companyId'=r.record_id AND r.result_json::jsonb->>'id'=r.record_id AND r.result_json::jsonb->>'status'='confirmed-existing' AND r.result_json::jsonb->>'version'='1'
 AND r.result_json::jsonb->'_manufacturingExistingPostingSetup'->>'kind'='manufacturing-existing-posting-setup'
 AND r.result_json::jsonb->'_manufacturingExistingPostingSetup'->>'companyId'=r.record_id
 AND r.result_json::jsonb->'_manufacturingExistingPostingSetup'->>'confirmationCommandId'=r.command_id
 AND r.result_json::jsonb->'_manufacturingExistingPostingSetup'->'mapping'->>'company_id'=r.record_id)) goods ON goods.command_id=receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'manufacturingSetupCommandId' AND goods.record_id=c.company_id AND goods.authority_json::jsonb->>'companyId'=c.company_id AND goods.result_json::jsonb->>'id'=c.company_id AND goods.result_json::jsonb->>'version'='1' AND goods.request_hash=receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'manufacturingSetupRequestHash'
 AND (goods.operation='commerce.manufacturing-posting-setup.initialize' OR goods.result_json::jsonb->'_manufacturingExistingPostingSetup'->'mapping'->>'chart_id'=receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'chartId' AND goods.result_json::jsonb->'_manufacturingExistingPostingSetup'->'mapping'->>'inventory_account_id'=receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'inventoryAccountId')
 WHERE NOT jsonb_exists(receipt.result_json::jsonb,'_initialPhysicalStockCount') AND jsonb_exists(receipt.result_json::jsonb,'_manufacturingInitialPhysicalStockCount') AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'kind'='manufacturing-initial-physical-stock-count'
 AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'countId'=c.id AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'companyId'=c.company_id
 AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'branchId'=c.branch_id AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'warehouseId'=c.warehouse_id AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'productId'=line.product_id AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'->>'countCommandId'=receipt.command_id
 AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1'
 AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0 AND c.quality_hold_id IS NULL
 AND (SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1) source JOIN inventory_counts c ON c.id=source.id AND c.company_id=source.company_id AND c.status='posted' JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.chart_id=source.source_proof->>'chartId' AND j.currency=source.source_proof->>'currency' AND j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted' AND j.is_opening=0
 WHERE c.net_value_minor>0 AND c.net_value_minor=(SELECT sum(line.counted_value_minor-line.snapshot_value_minor) FROM inventory_count_lines line WHERE line.count_id=c.id)
 AND (SELECT count(*) FROM finance_journal_lines line WHERE line.journal_id=j.id)=2
 AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'inventoryAccountId' AND line.debit_minor=c.net_value_minor AND line.credit_minor=0)
 AND EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'varianceAccountId' AND line.credit_minor=c.net_value_minor AND line.debit_minor=0)
 AND EXISTS(SELECT 1 FROM (WITH terminals AS(
 SELECT 'count'::text kind,count.id,count.company_id,count.status,count.version,
 event.reviewed_version+1 terminal_version,command.command_id
 FROM inventory_counts count JOIN inventory_count_decisions event ON event.count_id=count.id
 JOIN command_receipts command ON command.command_id=event.command_id AND command.record_id=count.id AND command.actor_id=event.actor_id
 AND command.operation='inventory.count.'||event.action AND command.authority_json::jsonb->>'companyId'=count.company_id
 AND command.result_json::jsonb->>'status'=count.status AND command.result_json::jsonb->>'version'=(event.reviewed_version+1)::text
 WHERE count.status IN ('posted','cancelled','rejected') AND event.action=CASE count.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END
 UNION ALL SELECT 'landed-cost',cost.id,cost.company_id,cost.status,cost.version,event.reviewed_version+1,command.command_id
 FROM inventory_landed_costs cost JOIN inventory_landed_cost_events event ON event.landed_cost_id=cost.id
 JOIN command_receipts command ON command.command_id=event.command_id AND command.record_id=cost.id AND command.actor_id=event.actor_id
 AND command.operation='inventory.landed_cost.'||event.action AND command.authority_json::jsonb->>'companyId'=cost.company_id
 AND command.result_json::jsonb->>'status'=cost.status AND command.result_json::jsonb->>'version'=(event.reviewed_version+1)::text
 WHERE cost.status IN ('posted','cancelled','rejected') AND event.action=CASE cost.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END
 ) SELECT terminal.* FROM terminals terminal WHERE terminal.terminal_version<=terminal.version AND EXISTS(
 WITH RECURSIVE versions(version) AS(SELECT terminal.terminal_version UNION
 SELECT binding.next_version FROM versions JOIN (SELECT source.company_id,source.target_type,source.target_id,
 source.expected_target_version previous_version,source.expected_target_version+1 next_version,
 command.command_id,to_jsonb(event) event_image,to_jsonb(source) set_image
 FROM attachment_sets source JOIN attachment_events event ON event.company_id=source.company_id AND event.set_id=source.id AND event.action='set.bound'
 JOIN command_receipts command ON command.command_id=source.bound_command_id AND command.record_id=source.target_id AND command.actor_id=event.actor_id
 AND command.authority_json::jsonb->>'companyId'=source.company_id
 WHERE source.state='bound' AND event.details_json::jsonb->>'bindingId'=source.target_id AND
 ((command.operation='attachment.record.bind' AND source.component_kind='record' AND event.details_json::jsonb->>'bindingType'='record_document'
 AND command.result_json::jsonb->>'setId'=source.id AND command.result_json::jsonb->>'version'=(source.expected_target_version+1)::text)
 OR(command.operation='attachment.compatibility.bind' AND event.details_json::jsonb->>'bindingType'='legacy_upload_adapter'
 AND command.result_json::jsonb->>'attachmentSetId'=source.id AND command.result_json::jsonb->>'ownerVersion'=(source.expected_target_version+1)::text))) binding ON binding.previous_version=versions.version
 AND binding.company_id=terminal.company_id AND binding.target_id=terminal.id
 AND (terminal.kind='count' AND binding.target_type IN ('stock_count','inventory_stock_count') OR terminal.kind='landed-cost' AND binding.target_type='landed_cost')
 WHERE binding.next_version<=terminal.version) SELECT 1 FROM versions WHERE version=terminal.version)) terminal WHERE terminal.kind='count' AND terminal.id=c.id AND terminal.company_id=c.company_id AND terminal.status=c.status AND terminal.version=c.version)) source ON source.id=root.id AND source.company_id=root.company_id) AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=workspace_records.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=workspace_records.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='manufacturing' AND purchase.package_version=1))) AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=workspace_records.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=workspace_records.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='manufacturing' AND purchase.package_version=1))) AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=workspace_records.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=workspace_records.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='pos' AND purchase.package_version=1)) OR (EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=workspace_records.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=workspace_records.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='pms' AND purchase.package_version=1)) OR (EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=workspace_records.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=workspace_records.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='sales' AND purchase.package_version=1)) OR (EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=workspace_records.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=workspace_records.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='manufacturing' AND purchase.package_version=1))) AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=workspace_records.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=workspace_records.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='manufacturing' AND purchase.package_version=1))) AND FALSE AND EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=workspace_records.company_id AND f.module_id='inventory' AND (CASE WHEN f.module_id='legal' THEN f.rollout_state IN ('pilot','available') AND FALSE ELSE (f.rollout_state='pilot' OR (f.rollout_state='available' AND FALSE)) END)) AND EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=workspace_records.company_id AND f.module_id='finance' AND (CASE WHEN f.module_id='legal' THEN f.rollout_state IN ('pilot','available') AND FALSE ELSE (f.rollout_state='pilot' OR (f.rollout_state='available' AND FALSE)) END))) ELSE EXISTS(SELECT 1 FROM finance_journals initial_finance_document WHERE initial_finance_document.id=workspace_records.id AND initial_finance_document.company_id=workspace_records.company_id AND EXISTS(SELECT 1 FROM (SELECT initial_finance_document.id,initial_finance_document.company_id UNION SELECT p.id,p.company_id FROM finance_book_postings b JOIN finance_journals p ON p.id=b.primary_journal_id AND p.company_id=initial_finance_document.company_id WHERE b.secondary_journal_id=initial_finance_document.id) root JOIN (SELECT source.company_id,c.journal_id id FROM (SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'_initialPhysicalStockCount' source_proof
 FROM inventory_counts c JOIN command_receipts receipt ON receipt.operation='inventory.count.create' AND receipt.record_id=c.id AND c.id='count-'||receipt.command_id AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id
 JOIN inventory_count_lines line ON line.count_id=c.id
 JOIN command_receipts goods ON goods.command_id=receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'goodsSetupCommandId' AND goods.operation='commerce.sales-goods-posting-setup.initialize' AND goods.record_id=c.company_id AND goods.authority_json::jsonb->>'companyId'=c.company_id AND goods.result_json::jsonb->>'id'=c.company_id AND goods.result_json::jsonb->>'status'='configured' AND goods.result_json::jsonb->>'version'='1' AND goods.request_hash=receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'goodsSetupRequestHash'
 WHERE jsonb_exists(receipt.result_json::jsonb,'_initialPhysicalStockCount') AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'kind'='initial-physical-stock-count'
 AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'countId'=c.id AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'companyId'=c.company_id
 AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'branchId'=c.branch_id AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'warehouseId'=c.warehouse_id AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'productId'=line.product_id AND receipt.result_json::jsonb->'_initialPhysicalStockCount'->>'countCommandId'=receipt.command_id
 AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1'
 AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0 AND c.quality_hold_id IS NULL
 AND (SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1) source JOIN inventory_counts c ON c.id=source.id AND c.company_id=source.company_id AND c.status='posted' JOIN finance_journals j ON j.id=c.journal_id AND j.company_id=c.company_id AND j.chart_id=source.source_proof->>'chartId' AND j.currency=source.source_proof->>'currency' AND j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted' AND j.is_opening=0) source ON source.id=root.id AND source.company_id=root.company_id) AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=initial_finance_document.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=initial_finance_document.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='sales' AND purchase.package_version=1))) AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=initial_finance_document.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=initial_finance_document.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='pos' AND purchase.package_version=1)) OR (EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=initial_finance_document.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=initial_finance_document.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='pms' AND purchase.package_version=1)) OR (EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=initial_finance_document.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=initial_finance_document.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='sales' AND purchase.package_version=1)) OR (EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=initial_finance_document.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=initial_finance_document.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='manufacturing' AND purchase.package_version=1))) AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=initial_finance_document.company_id AND starts_at<=records_search_actor.now_ms AND state<>'inactive'
      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=initial_finance_document.company_id AND retained_at<=records_search_actor.now_ms
        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='sales' AND purchase.package_version=1))) AND FALSE AND EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=initial_finance_document.company_id AND f.module_id='inventory' AND (CASE WHEN f.module_id='legal' THEN f.rollout_state IN ('pilot','available') AND FALSE ELSE (f.rollout_state='pilot' OR (f.rollout_state='available' AND FALSE)) END)) AND EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=initial_finance_document.company_id AND f.module_id='finance' AND (CASE WHEN f.module_id='legal' THEN f.rollout_state IN ('pilot','available') AND FALSE ELSE (f.rollout_state='pilot' OR (f.rollout_state='available' AND FALSE)) END))) END)))"##;

/// `assignmentReadSql` with the three positional binds replaced by the
/// materialized actor CTE columns, in the original bind order.
fn assignment_sql() -> String {
    let mut parts = ASSIGNMENT_TS_SQL.split('?');
    let head = parts
        .next()
        .expect("pinned assignment predicate has a head");
    let rest: Vec<&str> = parts.collect();
    assert_eq!(
        rest.len(),
        3,
        "assignment predicate has exactly three binds"
    );
    format!(
        "{}{}{}{}{}{}{}",
        head,
        "records_search_actor.assignment_id",
        rest[0],
        "records_search_actor.user_id",
        rest[1],
        "records_search_actor.role",
        rest[2]
    )
}

/// Adapt the byte-pinned dev filter instead of duplicating it. The pinned file
/// was generated as `initialPhysicalStockFinanceJournalFilter('j','now_ms')`:
/// `j` is both the outer journal alias (outer references) and an inner
/// `JOIN finance_journals j` alias. A global token replace would corrupt those
/// inner joins, so the transformer tracks parenthesis depth and only replaces
/// `j` outside a live inner-join scope; `now_ms` is unambiguous.
fn row_filter_dev_sql() -> String {
    const PINNED: &str = include_str!("initial_stock_journal_filter.sql");
    const INNER_J_DECLARATION: &str = "JOIN finance_journals j ";
    // The file keeps its final newline for the byte-pin test; the generated
    // predicate itself is the trimmed text.
    let pinned = PINNED.trim_end();
    let mut out = String::with_capacity(pinned.len() + 1024);
    let bytes = pinned.as_bytes();
    let mut depth: usize = 0;
    let mut inner_depths: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if pinned[i..].starts_with(INNER_J_DECLARATION) {
            inner_depths.push(depth);
            out.push_str(INNER_J_DECLARATION);
            i += INNER_J_DECLARATION.len();
            continue;
        }
        match bytes[i] {
            b'(' => {
                depth += 1;
                out.push('(');
                i += 1;
            }
            b')' => {
                out.push(')');
                i += 1;
                depth = depth.saturating_sub(1);
                inner_depths.retain(|d| *d <= depth);
            }
            b if b.is_ascii_alphanumeric() || b == b'_' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let token = &pinned[start..i];
                match token {
                    "j" if inner_depths.is_empty() => out.push_str("workspace_records"),
                    "now_ms" => out.push_str("records_search_actor.now_ms"),
                    _ => out.push_str(token),
                }
            }
            _ => {
                assert!(bytes[i].is_ascii(), "pinned owner SQL is ASCII");
                out.push(bytes[i] as char);
                i += 1;
            }
        }
    }
    out
}

/// The exact Finance WHERE fragment for Records Search. Returns trusted SQL
/// with no bind placeholders; the caller supplies the actor CTE and Company
/// scope. See the module contract above for the bind and mode semantics.
pub fn records_search_finance_predicate(
    enforced: bool,
    accounting_history: bool,
    production: bool,
) -> String {
    let filter = if production {
        format!("(SELECT {ROW_FILTER_PRODUCTION_SQL} FROM records_search_actor)")
    } else {
        format!(
            "(SELECT {} FROM records_search_actor)",
            row_filter_dev_sql()
        )
    };
    let authority = assignment_sql();
    let purchase = if enforced && !accounting_history {
        "FALSE"
    } else {
        "TRUE"
    };
    let ordinary_authority = if enforced {
        format!(" AND {authority}")
    } else {
        String::new()
    };
    format!(
        "(module_id<>'finance' OR ((CASE WHEN {INITIAL_STOCK_SQL} THEN {filter} AND {authority} ELSE {filter} AND {purchase}{ordinary_authority} END)))"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn hex(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn dev_row_filter_is_the_exact_authoritative_transform() {
        let sql = row_filter_dev_sql();
        assert_eq!(sql.len(), 48287);
        assert_eq!(
            hex(Sha256::digest(sql.as_bytes())),
            "ce299e4ce59152c33c6761387ab4284d1804ca8ba4b90f5bbd699ed3943f2e68"
        );
        assert!(!sql.contains('?'));
        assert!(!sql.contains("records_search_actor.now_ms."));
    }

    #[test]
    fn production_row_filter_keeps_scheduled_guards() {
        assert!(ROW_FILTER_PRODUCTION_SQL.contains("company_feature_rollouts"));
        assert_eq!(
            hex(Sha256::digest(ROW_FILTER_PRODUCTION_SQL.as_bytes())),
            "dfc8dc61d21db204c90811b6a239ae66e95a855594e0141648409323a9742d3e"
        );
    }

    #[test]
    fn assignment_uses_the_materialized_actor_without_binds() {
        let sql = assignment_sql();
        assert_eq!(ASSIGNMENT_TS_SQL.matches('?').count(), 3);
        assert!(!sql.contains('?'));
        assert!(sql.contains("g.id=records_search_actor.assignment_id"));
        assert!(sql.contains("g.user_id=records_search_actor.user_id"));
        assert!(sql.contains("g.role=records_search_actor.role"));
        assert!(sql.contains("g.company_id=workspace_records.company_id"));
        assert!(sql.contains(
            "iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)"
        ));
    }

    #[test]
    fn every_mode_keeps_initial_marker_and_assignment_gate() {
        for enforced in [false, true] {
            for accounting_history in [false, true] {
                for production in [false, true] {
                    let sql =
                        records_search_finance_predicate(enforced, accounting_history, production);
                    assert!(sql.starts_with("(module_id<>'finance' OR ((CASE WHEN "));
                    assert!(sql.contains(INITIAL_STOCK_SQL));
                    assert!(sql.contains("g.id=records_search_actor.assignment_id"));
                    assert!(sql.contains(" AND (SELECT "));
                    assert!(!sql.contains('?'));
                    assert!(!sql.contains('$'));
                    assert!(!sql.contains("finance_search_clock"));
                }
            }
        }
    }

    #[test]
    fn shadow_and_disabled_omit_ordinary_purchase_and_assignment() {
        for enforced in [false] {
            for accounting_history in [false, true] {
                let sql = records_search_finance_predicate(enforced, accounting_history, false);
                assert!(sql.contains(" ELSE (SELECT "));
                assert!(sql.contains(" AND TRUE END)))"));
                assert!(!sql.contains(" AND FALSE AND "));
            }
        }
    }

    #[test]
    fn enforced_applies_accounting_history_and_assignment_to_ordinary_rows() {
        let allowed = records_search_finance_predicate(true, true, false);
        assert!(allowed.contains(" AND TRUE AND EXISTS(SELECT 1 FROM assignments g "));
        let denied = records_search_finance_predicate(true, false, false);
        assert!(denied.contains(" AND FALSE AND EXISTS(SELECT 1 FROM assignments g "));
    }

    #[test]
    fn production_flag_selects_the_scheduled_filter() {
        let dev = records_search_finance_predicate(false, false, false);
        let prod = records_search_finance_predicate(false, false, true);
        assert!(!dev.contains("company_feature_rollouts"));
        assert!(prod.contains("company_feature_rollouts"));
    }
}
