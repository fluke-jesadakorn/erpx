import { productionScope } from '@/data/production-scope';
export const pilotModuleIds = productionScope.modules;

export type FeatureState = 'Available' | 'Setup required' | 'Pilot' | 'Coming Soon' | 'Prototype';
export type RolloutState = 'off' | 'pilot' | 'available';

const pilotIds = new Set<string>(pilotModuleIds);
const prototypeIds = new Set(['agent', 'ecommerce']);

export function isPilotModule(moduleId: string) { return pilotIds.has(moduleId); }
export function isReleaseModule(moduleId: string) { return isPilotModule(moduleId) || moduleId === 'legal'; }

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
  if (!isReleaseModule(input.moduleId)) return 'Coming Soon';
  if (input.moduleId === 'legal' && input.production && (!input.productionAccepted || !input.businessAccepted)) return 'Coming Soon';
  if (input.rollout === 'available' && (!input.productionAccepted || !input.businessAccepted)) return 'Coming Soon';
  if (!input.setupReady) return 'Setup required';
  if (input.rollout === 'available' && input.productionAccepted && input.businessAccepted) return 'Available';
  return 'Pilot';
}
