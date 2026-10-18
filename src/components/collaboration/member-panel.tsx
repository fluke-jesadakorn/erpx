"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { useRouter } from "@/lib/shared/destination-router";
import { Dialog } from "@/components/ui/dialog";
import { RevokeCompanyAccess } from "@/components/identity_access/revoke-company-access";
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
