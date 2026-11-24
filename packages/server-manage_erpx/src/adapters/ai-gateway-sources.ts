import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** One canonical beta_bootstrap scope read shared by the fence and the scope caller. */
export const betaBootstrapScope = 'SELECT 1 FROM beta_bootstrap h WHERE h.singleton=1 AND h.holding_id=?' as const;
/** Native source fragments retain the bootstrap fence inside the caller's atomic INSERT. */
export function aiGatewayBootstrapSources(db:SqlDatabase){return {
 scopeFence:`EXISTS(${betaBootstrapScope})`,
 statementSource:'FROM beta_bootstrap h WHERE h.singleton=1 AND h.holding_id=?',
 async binding(){return db.prepare('SELECT holding_id FROM beta_bootstrap WHERE singleton=1').first<{holding_id:string}>();},
 async scope(holdingId:string){return !!await db.prepare(betaBootstrapScope).bind(holdingId).first();},
};}
