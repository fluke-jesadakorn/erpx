"use client";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";

import { Button } from "@/components/ui/foundation";

import { MessageCircle, ExternalLink } from "lucide-react";
import { Dialog } from "@/components/ui/dialog";
import type { OrganizationOverview } from "@/lib/organization";
import { assignedLevel, type OrganizationGraph } from "@/lib/organization-chart";
import styles from "./organization-chart.module.css";

export function OrganizationPersonDetails({
  base,
  data,
  graph,
  personId,
  onClose,
  onTeam,
}: {
  base: string;
  data: OrganizationOverview;
  graph: OrganizationGraph;
  personId: string | null;
  onClose: () => void;
  onTeam: (id: string) => void;
}) {
  const entry = personId ? graph.entries.get(personId) : null;
  const person = entry?.person;
  const parentId = personId ? graph.parents.get(personId) : null;
  return (
    <Dialog
      open={Boolean(entry)}
      onClose={onClose}
      title={person?.legal_name ?? "Staff details"}
      presentation="responsive-panel"
    >
      {person && entry && (
        <div className={styles.personDetails}>
          <p className={styles.detailTitle}>{entry.title}</p>
          <dl className={styles.facts}>
            <div>
              <dt>Email</dt>
              <dd className={styles.detailEmail}>{person.email || "No email"}</dd>
            </div>
            <div>
              <dt>Level</dt>
              <dd>
                {assignedLevel(person.level_number) < 7
                  ? "Level " +
                    person.level_number +
                    (person.level_name ? " · " + person.level_name : "")
                  : "Level not assigned"}
              </dd>
            </div>
            <div>
              <dt>Branch</dt>
              <dd>
                {data.nodes.find((n) => n.id === person.branch_id)?.display_name ??
                  "Not shown / company-wide"}
              </dd>
            </div>
            <div>
              <dt>Department</dt>
              <dd>
                {data.nodes.find((n) => n.id === person.department_id)?.display_name ??
                  "Not shown / unassigned"}
              </dd>
            </div>
            <div>
              <dt>Reports to</dt>
              <dd>
                {parentId
                  ? graph.entries.get(parentId)!.person.legal_name
                  : (entry.boundary ??
                    (entry.isCeo && !person.manager_person_id
                      ? "Company leadership · no manager"
                      : "Reporting relationship needs review"))}
              </dd>
            </div>
            <div>
              <dt>Status</dt>
              <dd>{person.status}</dd>
            </div>
            <div>
              <dt>Direct reports in this view</dt>
              <dd>{graph.children.get(person.engagement_id)?.length ?? 0}</dd>
            </div>
          </dl>
          {entry.issues.length > 0 && (
            <div className={styles.issuePanel}>
              <strong>Profile needs setup</strong>
              <ul>
                {entry.issues.map((issue, i) => (
                  <li key={i}>{issue}</li>
                ))}
              </ul>
            </div>
          )}
          <div className={styles.detailActions}>
            <Button
              variant="primary"
              type="button"
              onClick={() => {
                onClose();
                onTeam(person.engagement_id);
              }}
            >
              View team
            </Button>
            <UiActionLink variant="secondary" href={base + "/people/" + person.person_id}>
              <ExternalLink size={16} aria-hidden="true" />
              {data.canManage ? "Open profile to edit" : "View profile"}
            </UiActionLink>
            {person.user_id && (
              <UiActionLink
                variant="secondary"
                href={base + "/inbox?person=" + encodeURIComponent(person.user_id)}
              >
                <MessageCircle size={16} aria-hidden="true" />
                Message
              </UiActionLink>
            )}
            {data.nodes
              .filter(
                (n) =>
                  [person.branch_id, person.department_id].includes(n.id) &&
                  data.authorizedChannelNodes?.includes(n.id),
              )
              .map((n) => (
                <UiActionLink
                  variant="secondary"
                  key={n.id}
                  href={base + "/inbox?room=" + encodeURIComponent("org-channel-" + n.id)}
                >
                  {n.display_name} channel
                </UiActionLink>
              ))}
          </div>
        </div>
      )}
    </Dialog>
  );
}
