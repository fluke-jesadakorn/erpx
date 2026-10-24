import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {one} from '@erpx/server-runtime/public/runtime/sql';
export function posStripeBindingFacts(db:SqlDatabase){return {slug:(holdingId:string)=>one(db,'SELECT slug FROM holding_bindings WHERE holding_id=?',holdingId)};}
