/** Typed GPO facts; providers use the caller's transaction without acquiring runtime. */
export type {GpoRuleRow,EffectiveRuleRow,Position} from '@erpx/server-identity_access/domain/gpo-rules';
export type {Input,Connection,Job,Claim,Ownership,Desired} from '@erpx/server-identity_access/domain/position-gpo';
