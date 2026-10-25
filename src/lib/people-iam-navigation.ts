/** Legacy Company routes have a single destination. Platform routes retain their console. */
export function peopleIamDestination(section: string) {
  if (section === "users" || section === "overview") return "people";
  if (section === "groups" || section === "mappings")
    return "technology?view=organization";
  if (section === "reviews") return "work?type=people_lifecycle";
  return `people/settings?section=${encodeURIComponent(section)}`;
}
