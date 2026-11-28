// Retired: the TS capability-decision adapter (readEffectiveCapability /
// readCommandCapability) had no production consumer. Native
// PgCapabilityDecisionReader::decide and the purchase command boundary own the
// decision and command-time authority; their oracle is
// tests/unit/commercial/rust-capability-decision.test.rs.
export { effectiveCapabilityDecision, capabilityRevisionSource } from "@erpx/commercial-server/domain/capability-decision";
export type { CapabilityGate, EffectiveCapabilityInput, EffectiveCapabilityDecision } from "@erpx/commercial-server/domain/capability-decision";
