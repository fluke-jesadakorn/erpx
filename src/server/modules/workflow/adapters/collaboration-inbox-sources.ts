import type {SqlDatabase} from '../../runtime/public/runtime/types';export function collaborationInboxSources(db:SqlDatabase){return {async failedMetrics(){return db.prepare("SELECT count(*)::int count FROM outbox_events WHERE dead_letter_at IS NOT NULL")
    .first<{count:number}>();}};}
