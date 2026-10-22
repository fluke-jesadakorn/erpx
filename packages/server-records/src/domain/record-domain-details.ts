export const domainRecordKinds = ['agent_pool', 'brand_draft', 'brand_version', 'agent_version', 'agent_assignment', 'manufacturing_spec', 'planning_scenario', 'ecommerce_listing', 'ecommerce_cart', 'ecommerce_channel', 'learning_course', 'benefit_plan', 'talent_requisition', 'talent_candidate'] as const;
export type DomainRecordKind = typeof domainRecordKinds[number];
export function isDomainRecordKind(kind: string): kind is DomainRecordKind { return domainRecordKinds.some(value => value === kind); }
