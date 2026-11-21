/** Holding aliases resolve to their owner workspace; Platform retains its console. */
export function peopleIamDestination(section: string) {
  if (section === "users" || section === "overview" || section === "my-access") return "technology?view=resources&section=accounts";
  if (section === "connections") return "technology?view=directory&section=connections";
  if (section === "groups" || section === "mappings") return "technology?view=settings&section=groups";
  return `technology?view=settings&section=${encodeURIComponent(section)}`;
}
