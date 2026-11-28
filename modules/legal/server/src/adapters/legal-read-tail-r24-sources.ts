import "server-only";
/** Byte-identical Legal hold-guard read prefix shared by preservation and service owners (round 24).
 * Text only: every call site still prepares and executes its own statement with unchanged binds. */
export const legalHoldTargetGuardPrefix = "SELECT 1 FROM legal_hold_targets target JOIN legal_holds hold ON hold.id=target.hold_id WHERE ";
