import 'server-only';import type {PoolClient} from 'pg';import type {TechnologyStore} from '../../identity_access/public/technology-contracts';
export function technologyProvisioningEffects(c:PoolClient):Pick<TechnologyStore,'appendAppliedOutbox'>{return {appendAppliedOutbox:async(facts)=>{const{op,observedNow}=facts;return void await c.query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES($1,$1,'people.provision.applied','people_provision',$2,1,$3,$4) ON CONFLICT DO NOTHING",
        [
          "provision:" + op.id,
          op.id,
          JSON.stringify({ id: op.id, companyId: op.company_id }),
          observedNow,
        ],
      );}};}
