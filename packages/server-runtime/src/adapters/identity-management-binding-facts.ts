import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import { holdingBindingsSql } from './read-tail-sql';
export function diagnosedHoldingBinding(db: SqlDatabase, slug: string) { return db.prepare(holdingBindingsSql('holding_id,status',' WHERE slug=?')).bind(slug).first<{holding_id: string; status: string}>(); }
export function onboardingSlugBinding(db: SqlDatabase, slug: string) { return db.prepare(holdingBindingsSql('1',' WHERE slug=?')).bind(slug).first(); }
