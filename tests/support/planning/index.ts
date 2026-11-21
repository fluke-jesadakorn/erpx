/**
 * Fixture-only oracle for the retired TypeScript Planning SQL writers.
 *
 * The production Planning scenario/resource command adapters, applications and
 * barrels were retired in favour of the native owner (see
 * `tests/integration/planning/rust-planning-command-postgres.test.rs` and the
 * `tests/unit/planning/rust-planning-*.test.rs` suites). The PostgreSQL
 * integration suite still needs the exact pre-cutover effects to seed state and
 * to exercise cross-owner purchase, supply, Finance and evidence behaviour.
 * The retired implementations therefore live here as test fixtures only;
 * production code must never import this barrel.
 */
export {
  createPlanningScenario,
  cancelPlanningScenario,
  reviewPlanningRecommendation,
  productionWorkspace,
} from "./legacy/scenarios";
export {
  saveProductionResource,
  saveProductionResourceCalendar,
} from "./legacy/resources";
export { productionCommandSources } from "./legacy/production-command-sources";
export {
  createPlanningScenarioApplication,
  type PlanningScenarioRepository,
  type PlanningScenarioPorts,
} from "./legacy/application-scenarios";
export {
  createPlanningResourceApplication,
  type PlanningResourceRepository,
  type PlanningResourcePorts,
} from "./legacy/application-resources";
