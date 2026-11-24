import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {betaSlotPointSql} from './beta-slot-point-sql';
export function ownerOnboardingSlot(db:SqlDatabase,userId:string){return db.prepare(
    betaSlotPointSql('slug,company_name,branch_name,state','owner_user_id'),
  )
    .bind(userId)
    .first<{
      slug: string;
      company_name: string;
      branch_name: string;
      state: string;
    }>();}
