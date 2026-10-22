/** Mounted route segment for a PMS section identity. */
export function pmsSectionPath(section: string): string {
  return section === "inventory" ? "minibar" : section;
}
