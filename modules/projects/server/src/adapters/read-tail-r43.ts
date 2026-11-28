/** Byte-identical dependency-projection head shared by the Projects executive
 * milestone search probe and owner cohort. Only the caller's alias and trailing
 * predicate vary; the relation, gap and bind order stay exactly as before.
 * Text only: nothing is cached, prepared or executed here. */
export const projectDependencyRowsHead = `SELECT 1 FROM staff_project_milestone_dependencies `;
