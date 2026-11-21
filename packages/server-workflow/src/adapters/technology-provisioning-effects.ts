import 'server-only';
import {nativeWorkflowWrite} from './native-workflow-write';import type {PoolClient} from 'pg';import type {TechnologyStore} from '@erpx/server-identity_access/public/technology-contracts';
export function technologyProvisioningEffects(c:PoolClient):Pick<TechnologyStore,'appendAppliedOutbox'>{return {appendAppliedOutbox:async(facts)=>{const{op,observedNow}=facts;return void await nativeWorkflowWrite('wf_native_tech_provision_outbox', ["provision:" + op.id, op.id, JSON.stringify({ id: op.id, companyId: op.company_id }), observedNow, "provision:" + op.id]).run();}};}
