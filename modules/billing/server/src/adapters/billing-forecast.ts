import {nativeBillingOwnerQuery} from '@erpx/billing-server/adapters/native-billing-owner-write';
import 'server-only';
import type {PoolClient} from 'pg';
import type {PostgresDatabase} from '@erpx/platform-runtime-server/public/runtime/postgres';
import {readSeatForecastBaseline as calculate,type SeatForecastRepository} from '@erpx/billing-server/application/forecast';
import type * as D from '@erpx/billing-server/domain/forecast';
export type {SeatForecastBaseline} from '@erpx/billing-server/domain/forecast';
export function nativeSeatForecastRepository(c:PoolClient):SeatForecastRepository{return {currentPrice:async(value0)=>(await nativeBillingOwnerQuery(c,'forecast_current_price',[value0])) as {rows:D.CurrentPriceRow[];rowCount:number|null},
futurePrice:async(value0)=>(await nativeBillingOwnerQuery(c,'forecast_future_price',[value0])) as {rows:D.FuturePriceRow[];rowCount:number|null},
usageSnapshot:async(value0,value1)=>(await nativeBillingOwnerQuery(c,'forecast_usage_snapshot',[value0,value1])) as {rows:D.UsageSnapshotRow[];rowCount:number|null},
retainedStorageTotals:async(value0,value1,value2)=>(await nativeBillingOwnerQuery(c,'forecast_retained_storage_totals',[value0,value1,value2])) as {rows:D.RetainedStorageTotalsRow[];rowCount:number|null}}}
export async function readSeatForecastBaseline(db:PostgresDatabase,holdingId:string,now=Date.now()){const c=await db.pool.connect();try{return await calculate(nativeSeatForecastRepository(c),holdingId,now)}finally{c.release()}}
