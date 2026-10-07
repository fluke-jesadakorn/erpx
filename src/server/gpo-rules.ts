/** Compatibility entry point; legacy implementation belongs to identity_access. */
export {canManageGpoRules,effectiveGpoRules,gpoRuleMatchesPosition,gpoRuleSourceId,gpoRulesOverview,markPositionGpoNotRequired,newGpoRuleId,previewGpoRule,previewRemoveGpoRule,publishGpoRule,readGpoRules,removeGpoRule,retryGpoRuleConfig} from "@/server/modules/identity_access/adapters/gpo-rules";
export type {EffectiveRuleRow,GpoRuleRow} from "@/server/modules/identity_access/adapters/gpo-rules";
