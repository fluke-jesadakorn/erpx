import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {one} from '@erpx/platform-runtime-server/public/runtime/sql';
import { holdingBindingsSql } from './read-tail-sql';
export function posStripeBindingFacts(db:SqlDatabase){return {slug:(holdingId:string)=>one(db,holdingBindingsSql('slug',' WHERE holding_id=?'),holdingId)};}
