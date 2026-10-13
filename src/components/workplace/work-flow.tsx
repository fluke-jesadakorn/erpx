import { ArrowRight, Users } from "lucide-react";
import type { WorkItem } from "@/lib/work_projection/work-item";
import { PersonAvatar } from "@/components/ui/person-avatar";
import styles from "./work-list.module.css";

export function WorkFlow({ item }: { item: WorkItem }) {
  const current = item.flow?.current;
  const next = item.flow?.next;
  const person = current?.primary;
  if (item.queue === "completed" && !current) return null;
  return <div className={styles.flow} aria-label="Work handoff">
    <span className={styles.owner}>
      {person ? <PersonAvatar name={person.name} small /> : <Users size={17} aria-hidden />}
      <span><small>{item.queue === "completed" ? "Completed" : "Currently with"}</small><strong>{person?.name ?? current?.teamName ?? item.responsibleQueue ?? "Unassigned"}</strong></span>
    </span>
    {current && <span className={styles.current} aria-current="step">{current.action}</span>}
    {next && <span className={styles.next}><ArrowRight size={15} aria-hidden /><span>Next: {next.action}<small>{next.primary?.name ?? next.teamName ?? "Unassigned"}</small></span></span>}
  </div>;
}
