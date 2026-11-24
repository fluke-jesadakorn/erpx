import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {commerceQualityOriginRows} from '@erpx/server-commerce/public/quality-purchase-sources';
import {qualitySourceRows} from './quality-source-rows-sql';
import {qualityLineageSeedSql,qualityLineageStepSql} from './quality-lineage-recursion-sql';
import {qualityReturnAnchorSql} from './quality-return-anchor-sql';
/** Raw retained quarantine and immutable decisions. The Inventory withdrawal
 * reader proves actual Sales return identity; a source_type alone never grants
 * a Count or Quality command. Invalid declared origins remain unresolved. */
const returnHoldSources=`WITH RECURSIVE anchors AS(
 ${qualityReturnAnchorSql("h.id,h.company_id source_company","inventory_quality_holds h"," WHERE h.source_type='customer_return'")}
 UNION ${qualityReturnAnchorSql("origin.hold_id,origin.company_id",`(${commerceQualityOriginRows}) origin`," WHERE origin.kind='customer-return'")}),
 lineage AS(${qualityLineageSeedSql("anchors.id,anchors.source_company","anchors")} UNION ALL
 ${qualityLineageStepSql("child.id,parent.source_company","parent")} JOIN inventory_quality_holds original ON original.id=parent.id
 JOIN inventory_counts count ON count.id=original.disposition_count_id AND count.quality_hold_id=original.id AND count.status='posted'
 JOIN inventory_quality_disposition_events event ON event.hold_id=original.id AND event.count_id=count.id AND event.action='post'
 JOIN inventory_quality_holds child ON child.id=original.id||'-remaining-'||count.id AND child.source_id=original.source_id||':remaining:'||count.id
 WHERE parent.depth<64)
 SELECT DISTINCT ON(h.id,lineage.source_company) h.*,lineage.source_company,lineage.depth source_depth FROM lineage JOIN inventory_quality_holds h ON h.id=lineage.id ORDER BY h.id,lineage.source_company,lineage.depth DESC`;
export const salesQualitySettlementSources={returnOrigins:commerceQualityOriginRows,holds:qualitySourceRows('inventory_quality_holds'),returnHoldSources,inspections:qualitySourceRows('inventory_quality_inspections'),dispositions:qualitySourceRows('inventory_quality_disposition_events'),cancellations:qualitySourceRows('inventory_quality_cancellations')} as const;
export function salesQualitySettlementSourceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_disposition_events,inventory_quality_cancellations,inventory_quality_specs,inventory_quality_spec_metrics IN SHARE MODE')];}
