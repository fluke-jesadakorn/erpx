"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { Dialog } from "@erpx/ui/components/dialog";
import { RevokeCompanyAccess } from "@erpx/feature-identity_access/components/revoke-company-access";
import type { ReactNode } from "react";
export function MemberPanel({
  base,
  person,
  canRevoke,
  children,
}: {
  base: string;
  person: { id: string; name: string };
  canRevoke: boolean;
  children?: ReactNode;
}) {
  const router = useRouter();
  return (
    <Dialog appearance="standard"
      open
      title={person.name}
      onClose={() => router.push(canonicalUiHref(`${base}/people`))}
    >
      {children ?? <p>This teammate can participate in the workspace.</p>}
      {canRevoke && (
        <RevokeCompanyAccess appearance="standard"
          base={base}
          userId={person.id}
          onRevoked={() => router.replace(canonicalUiHref(`${base}/people`))}
        />
      )}
    </Dialog>
  );
}
