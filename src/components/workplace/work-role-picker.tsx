"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button } from "@/components/ui/foundation";

import { useHydrated } from "@/components/ui/use-hydrated";
import { useState } from "react";
import { useRouter } from "next/navigation";
import { RoleSelect } from "./role-select";
import { useContextSwitch } from "./context-switch-provider";
import type { WorkAssignment } from "@/server/authorization";
import styles from "./account.module.css";

export function WorkRolePicker({
  base,
  options,
  revision,
  from,
  companyId,
  activeAssignmentId,
}: {
  base: string;
  options: (WorkAssignment & { href: string })[];
  revision: number;
  from?: string;
  companyId?: string;
  activeAssignmentId?: string | null;
}) {
  const ready = useHydrated();
  const router = useRouter();
  const { switchContext, busy } = useContextSwitch();
  const [error, setError] = useState("");
  return (
    <div className={styles.skin}>
      <RoleSelect
        assignments={options}
        value={activeAssignmentId ?? ""}
        disabled={!ready || busy}
        onChange={async (assignmentId) => {
          if (assignmentId === activeAssignmentId || busy) return;
          setError("");
          try {
            const company =
              companyId ??
              options.find((option) => option.id === assignmentId)?.companyId ??
              undefined;
            await switchContext({
              companyId: company,
              from,
              returnTo: from,
              action: async () => {
                const response = await fetch(`/api/v1${base}/assignments/active`, {
                  method: "POST",
                  headers: {
                    "content-type": "application/json",
                    "x-erpx-context-company": company ?? "",
                  },
                  body: JSON.stringify({ assignmentId, revision }),
                });
                const body = await response.json();
                if (!response.ok) throw Error(body.error?.message ?? "Role could not be selected.");
              },
            });
          } catch (cause) {
            setError(cause instanceof Error ? cause.message : "Try again.");
          }
        }}
      />
      {!options.length && (
        <p>No active roles are available. Ask an administrator to review your access.</p>
      )}
      {error && (
        <InlineNotice tone="danger" role="alert">
          {error}{" "}
          <Button appearance="standard" variant="ghost" type="button" onClick={() => router.refresh()}>
            Check again
          </Button>
        </InlineNotice>
      )}
    </div>
  );
}
