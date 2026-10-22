import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function commercialSeatCutoffDependency(db:SqlDatabase,start:number,end:number){return db.prepare(`SELECT EXISTS(SELECT 1 FROM people_access_cutoffs
      WHERE effective_at<? AND (restored_at IS NULL OR restored_at>?)
        AND (cancelled_at IS NULL OR cancelled_at>?)) dependency`).bind(end,start,start).first<{dependency:boolean}>();}
