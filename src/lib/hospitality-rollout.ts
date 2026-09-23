export const pilotModuleIds = [
  'organization', 'identity_access', 'people_payroll', 'operations',
  'procurement', 'inventory', 'finance', 'pos', 'pms',
  'collaboration',
] as const;

export type FeatureState = 'Available' | 'Setup required' | 'Pilot' | 'Coming Soon' | 'Prototype';
export type RolloutState = 'off' | 'pilot' | 'available';

const pilotIds = new Set<string>(pilotModuleIds);
const prototypeIds = new Set(['agent', 'ecommerce']);

export function isPilotModule(moduleId: string) { return pilotIds.has(moduleId); }

export function resolveFeatureState(input: {
  moduleId: string;
  production: boolean;
  rollout?: RolloutState;
  setupReady: boolean;
  productionAccepted: boolean;
  businessAccepted: boolean;
}): FeatureState {
  if (prototypeIds.has(input.moduleId) && input.production && !input.rollout) return 'Prototype';
  if (input.production && !input.rollout) return 'Coming Soon';
  if (input.rollout === 'off') return 'Coming Soon';
  if (!isPilotModule(input.moduleId) && input.rollout !== 'available') return 'Coming Soon';
  if (!input.setupReady) return 'Setup required';
  if (input.rollout === 'available' && input.productionAccepted && input.businessAccepted) return 'Available';
  return 'Pilot';
}
