import { lastAuditOwnerProjectionSources as commercialLastAuditProjectionSources } from "../../commercial/public/last-audit-owner-projection-sources";
import 'server-only';
import type {PoolClient} from 'pg';
import type {PostgresDatabase} from '@/server/modules/runtime/public/runtime/postgres';
import {readSeatForecastBaseline as calculate,type SeatForecastRepository} from '../application/forecast';
import type * as D from '../domain/forecast';
export type {SeatForecastBaseline} from '../domain/forecast';
export function nativeSeatForecastRepository(c:PoolClient):SeatForecastRepository{return {currentPrice:(value0)=>c.query<D.CurrentPriceRow>(`SELECT * FROM billing_target_prices
        WHERE rating_enabled=TRUE AND seat_terms_status='defined' AND base_monthly_satang=0
          AND included_seats=0 AND seat_monthly_satang IS NOT NULL
          AND storage_allowance_mode='per_paid_seat_time' AND effective_month<=$1
        ORDER BY effective_month DESC,id DESC LIMIT 1`,[value0]),
futurePrice:(value0)=>c.query<D.FuturePriceRow>(`SELECT * FROM billing_target_prices
        WHERE rating_enabled=TRUE AND seat_terms_status='defined' AND base_monthly_satang=0
          AND included_seats=0 AND seat_monthly_satang IS NOT NULL
          AND storage_allowance_mode='per_paid_seat_time' AND effective_month>$1
        ORDER BY effective_month ASC,id ASC LIMIT 1`,[value0]),
usageSnapshot:(value0,value1)=>c.query<D.UsageSnapshotRow>(`SELECT snapshot_id,refreshed_at,source_count,storage_coverage,
          active_seat_count,active_seat_observed_at FROM (${commercialLastAuditProjectionSources.billing_usage_snapshots}) billing_usage_snapshots
          WHERE holding_id=$1 AND month=$2`,[value0,value1]),
retainedStorageTotals:(value0,value1,value2)=>c.query<D.RetainedStorageTotalsRow>(`SELECT COUNT(*)::text row_count,
        COALESCE(SUM(quantity) FILTER (WHERE meter_kind='storage' AND active_at_period_end),0)::text active_original_bytes
       FROM (${commercialLastAuditProjectionSources.billing_usage_meters}) billing_usage_meters WHERE holding_id=$1 AND month=$2 AND snapshot_id=$3`,[value0,value1,value2])}}
export async function readSeatForecastBaseline(db:PostgresDatabase,holdingId:string,now=Date.now()){const c=await db.pool.connect();try{return await calculate(nativeSeatForecastRepository(c),holdingId,now)}finally{c.release()}}
