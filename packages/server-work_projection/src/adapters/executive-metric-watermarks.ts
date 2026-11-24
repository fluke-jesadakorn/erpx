import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';import {workCacheRevisionsSql} from './read-tail-sql';export async function executiveMetricWatermarks(sourceDb:SqlDatabase,sourceTables:string[]){return (await sourceDb.prepare(workCacheRevisionsSql(sourceTables))
      .bind(...sourceTables).all<{ table_name: string; revision: string }>()).results;}
