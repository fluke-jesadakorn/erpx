import 'server-only';
import summary from '@/data/hospitality-evidence.generated.json';

/** Formal delivery evidence, distinct from a company's rollout and setup. */
export function productionAccepted(moduleId: string) {
  const gates=summary[moduleId as keyof typeof summary];
  return Boolean(gates?.automated && gates.browser && gates.provider && gates.production);
}
