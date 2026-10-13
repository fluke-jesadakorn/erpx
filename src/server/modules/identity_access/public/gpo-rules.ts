/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { canManageGpoRules, readGpoRules, effectiveGpoRules, gpoRulesOverview, markPositionGpoNotRequired, gpoRuleMatchesPosition, previewGpoRule, previewRemoveGpoRule, publishGpoRule, removeGpoRule, retryGpoRuleConfig, gpoRuleSourceId, newGpoRuleId } from "@/server/modules/identity_access/adapters/gpo-rules";
export type { GpoRuleRow, EffectiveRuleRow } from "@/server/modules/identity_access/adapters/gpo-rules";
