import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Native source fragments retain the bootstrap fence inside the caller's atomic INSERT. */
export function aiGatewayBootstrapSources(db:SqlDatabase){return {
 scopeFence:'EXISTS(SELECT 1 FROM beta_bootstrap h WHERE h.singleton=1 AND h.holding_id=?)',
 statementSource:'FROM beta_bootstrap h WHERE h.singleton=1 AND h.holding_id=?',
 async binding(){return db.prepare('SELECT holding_id FROM beta_bootstrap WHERE singleton=1').first<{holding_id:string}>();},
 async scope(holdingId:string){return !!await db.prepare('SELECT 1 FROM beta_bootstrap h WHERE h.singleton=1 AND h.holding_id=?').bind(holdingId).first();},
};}
