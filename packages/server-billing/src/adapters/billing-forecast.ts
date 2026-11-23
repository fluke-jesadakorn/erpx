import {nativeCommercialOwnerRead,COMMERCIAL_RD_USAGE_METER,COMMERCIAL_RD_USAGE_SNAPSHOT} from '@erpx/server-commercial/adapters/native-owner-read';
import {nativeBillingOwnerQuery} from '@erpx/server-billing/adapters/native-billing-owner-write';
import 'server-only';
import type {PoolClient} from 'pg';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import {readSeatForecastBaseline as calculate,type SeatForecastRepository} from '@erpx/server-billing/application/forecast';
import type * as D from '@erpx/server-billing/domain/forecast';
export type {SeatForecastBaseline} from '@erpx/server-billing/domain/forecast';
export function nativeSeatForecastRepository(c:PoolClient):SeatForecastRepository{return {currentPrice:async(value0)=>(await nativeBillingOwnerQuery(c,'forecast_current_price',[value0])) as {rows:D.CurrentPriceRow[];rowCount:number|null},
futurePrice:async(value0)=>(await nativeBillingOwnerQuery(c,'forecast_future_price',[value0])) as {rows:D.FuturePriceRow[];rowCount:number|null},
usageSnapshot:async(value0,value1)=>c.query<D.UsageSnapshotRow>(`SELECT snapshot_id,refreshed_at,source_count,storage_coverage,
          active_seat_count,active_seat_observed_at FROM (${await nativeCommercialOwnerRead(COMMERCIAL_RD_USAGE_SNAPSHOT)}) billing_usage_snapshots
          WHERE holding_id=$1 AND month=$2`,[value0,value1]),
retainedStorageTotals:async(value0,value1,value2)=>c.query<D.RetainedStorageTotalsRow>(`SELECT COUNT(*)::text row_count,
        COALESCE(SUM(quantity) FILTER (WHERE meter_kind='storage' AND active_at_period_end),0)::text active_original_bytes
       FROM (${await nativeCommercialOwnerRead(COMMERCIAL_RD_USAGE_METER)}) billing_usage_meters WHERE holding_id=$1 AND month=$2 AND snapshot_id=$3`,[value0,value1,value2])}}
export async function readSeatForecastBaseline(db:PostgresDatabase,holdingId:string,now=Date.now()){const c=await db.pool.connect();try{return await calculate(nativeSeatForecastRepository(c),holdingId,now)}finally{c.release()}}
