import 'server-only';
import {ecommerceCommerceSettlementSources as commerce} from '@erpx/commerce-server/public/ecommerce-package-sources';
import {salesQualitySettlementSources as quality} from '@erpx/quality-server/adapters/sales-package-sources';
import {qualityLineageSeedSql,qualityLineageStepSql} from './quality-lineage-recursion-sql';
import {qualityLineageTerminalSql} from './quality-lineage-terminal-sql';
import {nativeSourceWhere} from './native-source-wrapper-sql';
import {qualityIdInSourceSql} from './quality-id-source-sql';
import {qualityReturnAnchorSql} from './quality-return-anchor-sql';
const origins=`${nativeSourceWhere(quality.returnOrigins,`native.kind='customer-return' AND native.owner_id ${qualityIdInSourceSql(commerce.returns, 'returned')}`)}`;
const holdIds=`WITH RECURSIVE anchors AS(
 ${qualityReturnAnchorSql("origin.hold_id id",`(${origins}) origin`)}
 UNION ${qualityReturnAnchorSql("hold.id","inventory_quality_holds hold",` WHERE hold.source_type='customer_return' AND (hold.source_id ${qualityIdInSourceSql(commerce.returnLines, 'line')} OR hold.source_id ${qualityIdInSourceSql(commerce.returnLots, 'lot')})`)}),
 lineage AS(${qualityLineageSeedSql("id","anchors")} UNION ALL ${qualityLineageStepSql("child.id","parent")} JOIN inventory_quality_holds original ON original.id=parent.id JOIN inventory_counts count ON count.id=original.disposition_count_id AND count.quality_hold_id=original.id AND count.status='posted' JOIN inventory_quality_disposition_events event ON event.hold_id=original.id AND event.count_id=count.id AND event.action='post' JOIN inventory_quality_holds child ON child.id=original.id||'-remaining-'||count.id AND child.source_id=original.source_id||':remaining:'||count.id WHERE parent.depth<64) ${qualityLineageTerminalSql("DISTINCT id")}`;
export const ecommerceQualitySettlementSources={...quality,returnOrigins:origins,returnHoldSources:`${nativeSourceWhere(quality.returnHoldSources,`native.id IN (${holdIds})`)}`} as const;
