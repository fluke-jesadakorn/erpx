"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { ActionAnchor as UiActionAnchor } from "@erpx/ui/components/foundation";

import { Button, Field, Select, TextInput } from "@erpx/ui/components/foundation";

import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { useRef, useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { businessRoles, businessRoleLabel } from "@erpx/feature-identity_access/lib/business-roles";
import { StatusBadge } from "@erpx/ui/components/primitives";
import { RevokeCompanyAccess } from "@erpx/feature-identity_access/components/revoke-company-access";
import { type UiAppearance } from "@erpx/ui/components/foundation";
export function AccessEditor({ appearance = "legacy",
  base,
  personId,
  hasAccount,
  assignments,
  canManage = true,
  userId,
  canRevokeCompany = false,
}: ({
  base: string;
  personId: string;
  hasAccount: boolean;
  assignments: Record<string, string | number | null>[];
  canManage?: boolean;
  userId?: string;
  canRevokeCompany?: boolean;
}) & { appearance?: UiAppearance }) {
  const router = useRouter(),
    attempt = useRef<{ payload: string; key: string } | null>(null);
  const [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [activationLink, setActivationLink] = useState(""),
    [pending, setPending] = useState(false),
    [role, setRole] = useState("employee");
  async function send(action: string, input: unknown) {
    const payload = JSON.stringify({ action, input });
    if (attempt.current?.payload !== payload)
      attempt.current = { payload, key: crypto.randomUUID() };
    setPending(true);
    setError("");
    try {
      const r = await workFetch(`/api/v1${base}/people/directory/${personId}/${action}`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": attempt.current.key,
        },
        body: JSON.stringify(input),
      });
      const v = (await r.json()) as {
        data: { token?: string; message?: string };
        error?: { message: string };
      };
      if (!r.ok) throw Error(v.error?.message ?? "Unable to update access.");
      if (v.data.message) setNotice(v.data.message);
      if (v.data.token) {
        setActivationLink(
          `${window.location.origin}/activate?token=${encodeURIComponent(v.data.token)}`,
        );
      }
      attempt.current = null;
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to update access.");
    } finally {
      setPending(false);
    }
  }
  return (
    <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
      <h2 data-ui-appearance={appearance}>Login & access</h2>
      <UiActionAnchor appearance={appearance} variant="secondary" href={canonicalUiHref(`${base}/iam/users`)}>
        Open Identity & access
      </UiActionAnchor>
      <p data-ui-appearance={appearance}>
        {hasAccount
          ? "Login account linked. Roles below control access to this workspace."
          : "No login account is linked to this employee."}
      </p>
      {error && <p data-ui-appearance={appearance} role="alert">{error}</p>}
      {assignments.map((a) => (
        <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-record-item" : "row"} key={String(a.id)}>
          <div data-ui-appearance={appearance} className="row-main">
            <strong data-ui-appearance={appearance}>{businessRoleLabel(String(a.role))}</strong>
            <p data-ui-appearance={appearance} className="muted">
              {String(a.scope_kind)} · {String(a.scope_node_id ?? a.company_id)}
            </p>
          </div>
          <StatusBadge appearance={appearance} status={String(a.access_status ?? (a.revoked_at ? "revoked" : "active"))} />
          {canManage && !a.revoked_at && a.role !== "holding_owner" && (
            <Button appearance={appearance}
              type="submit"
              variant="secondary"

              disabled={pending}
              onClick={() => void send("revoke", { assignmentId: a.id })}
            >
              Revoke role
            </Button>
          )}
        </div>
      ))}
      {canManage && (
        <FormDisclosure appearance={appearance}>
          <summary data-ui-appearance={appearance}>{hasAccount ? "Grant role" : "Invite to ERPX"}</summary>
          <form data-ui-appearance={appearance}
            className={appearance === "standard" ? "ui-form-grid" : "business-form"}
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send(hasAccount ? "grant" : "invite", {
                role: f.get("role"),
                scope: role === "it_admin" ? "holding" : f.get("scope"),
                expiresAt: f.get("expiresAt"),
              });
            }}
          >
            <Field appearance={appearance}>
              System role
              <Select appearance={appearance} name="role" value={role} onChange={(e) => setRole(e.target.value)}>
                {Object.keys(businessRoles).map((role) => (
                  <option data-ui-appearance={appearance} key={role} value={role}>
                    {businessRoleLabel(role)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance={appearance}>
              Access scope
              <Select appearance={appearance}
                name="scope"
                key={role === "it_admin" ? "company" : "optional"}
                disabled={role === "it_admin"}
                defaultValue={role === "it_admin" ? "holding" : "branch"}
              >
                <option data-ui-appearance={appearance} value="branch">Employee’s branch</option>
                <option data-ui-appearance={appearance} value="department">Employee’s department</option>
                <option data-ui-appearance={appearance} value="holding">All branches in this company</option>
              </Select>
            </Field>
            <Field appearance={appearance}>
              Access expires
              <TextInput appearance={appearance} name="expiresAt" type="date" required />
            </Field>
            <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={pending}>
              {hasAccount ? "Grant role" : "Create activation link"}
            </Button>
          </form>
        </FormDisclosure>
      )}
      {notice && <p data-ui-appearance={appearance} role="status">{notice}</p>}
      {activationLink && !hasAccount && (
        <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
          <Field appearance={appearance}>
            Activation link (expires in 48 hours)
            <TextInput appearance={appearance}
              value={activationLink}
              readOnly
              aria-describedby="activation-link-help"
              onFocus={(event) => event.currentTarget.select()}
            />
          </Field>
          <p data-ui-appearance={appearance} id="activation-link-help" className="muted">
            Copy this link into the approved sandbox mailbox or share it with the invited employee
            after independent access approval. It can be used only once and remains subject to the
            recorded review.
          </p>
        </div>
      )}
      {canRevokeCompany && userId && assignments.some((a) => a.access_status === "active") && (
        <RevokeCompanyAccess appearance={appearance} base={base} userId={userId} />
      )}
    </section>
  );
}
