import 'server-only';
import {ecommerceCommerceSettlementSources as commerce} from '../../commerce/public/ecommerce-package-sources';
import {salesQualitySettlementSources as quality} from './sales-package-sources';
const origins=`SELECT native.* FROM (${quality.returnOrigins}) native WHERE native.kind='customer-return' AND native.owner_id IN (SELECT id FROM (${commerce.returns}) returned)`;
const holdIds=`WITH RECURSIVE anchors AS(
 SELECT origin.hold_id id FROM (${origins}) origin
 UNION SELECT hold.id FROM inventory_quality_holds hold WHERE hold.source_type='customer_return' AND (hold.source_id IN (SELECT id FROM (${commerce.returnLines}) line) OR hold.source_id IN (SELECT id FROM (${commerce.returnLots}) lot))),
 lineage AS(SELECT id,0 depth FROM anchors UNION ALL SELECT child.id,parent.depth+1 FROM lineage parent JOIN inventory_quality_holds original ON original.id=parent.id JOIN inventory_counts count ON count.id=original.disposition_count_id AND count.quality_hold_id=original.id AND count.status='posted' JOIN inventory_quality_disposition_events event ON event.hold_id=original.id AND event.count_id=count.id AND event.action='post' JOIN inventory_quality_holds child ON child.id=original.id||'-remaining-'||count.id AND child.source_id=original.source_id||':remaining:'||count.id WHERE parent.depth<64) SELECT DISTINCT id FROM lineage`;
export const ecommerceQualitySettlementSources={...quality,returnOrigins:origins,returnHoldSources:`SELECT native.* FROM (${quality.returnHoldSources}) native WHERE native.id IN (${holdIds})`} as const;
