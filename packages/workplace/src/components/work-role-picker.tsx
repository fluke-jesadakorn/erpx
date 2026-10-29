"use client";
import {holdingFetch} from '@erpx/contracts/holding-fetch';
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button } from "@erpx/ui/components/foundation";

import { useHydrated } from "@erpx/ui/components/use-hydrated";
import { useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { RoleSelect } from "@erpx/workplace/components/role-select";
import { useContextSwitch } from "@erpx/workplace/components/context-switch-provider";
import type {WorkRoleDisplayChoice} from "@erpx/feature-identity_access/lib/work-role-display";
import styles from "@erpx/workplace/components/account.module.css";

export function WorkRolePicker({
  base,
  options,
  revision,
  from,
  companyId,
  activeAssignmentId,
}: {
  base: string;
  options: readonly WorkRoleDisplayChoice[];
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
                const response = await holdingFetch(`/api/v1${base}/assignments/active`, {
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
