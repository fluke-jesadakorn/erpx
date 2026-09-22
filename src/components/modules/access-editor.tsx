"use client";

import { workFetch } from "@/lib/work-fetch";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { businessRoles, businessRoleLabel } from "@/lib/business-roles";
import { StatusBadge } from "@/components/ui/primitives";
import { RevokeCompanyAccess } from "./revoke-company-access";
export function AccessEditor({
  base,
  personId,
  hasAccount,
  assignments,
  canManage = true,
  userId,
  canRevokeCompany = false,
}: {
  base: string;
  personId: string;
  hasAccount: boolean;
  assignments: Record<string, string | number | null>[];
  canManage?: boolean;
  userId?: string;
  canRevokeCompany?: boolean;
}) {
  const router = useRouter(),
    attempt = useRef<{ payload: string; key: string } | null>(null);
  const [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [pending, setPending] = useState(false),
    [role, setRole] = useState("employee");
  async function send(action: string, input: unknown) {
    const payload = JSON.stringify({ action, input });
    if (attempt.current?.payload !== payload)
      attempt.current = { payload, key: crypto.randomUUID() };
    setPending(true);
    setError("");
    try {
      const r = await workFetch(
        `/api/v1${base}/people/directory/${personId}/${action}`,
        {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": attempt.current.key,
          },
          body: JSON.stringify(input),
        },
      );
      const v = (await r.json()) as {
        data: { token?: string;message?:string };
        error?: { message: string };
      };
      if (!r.ok) throw Error(v.error?.message ?? "Unable to update access.");
      if(v.data.message)setNotice(v.data.message);
      attempt.current = null;
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to update access.");
    } finally {
      setPending(false);
    }
  }
  return (
    <section className="card">
      <h2>Login & access</h2><a className="button secondary" href={`${base}/iam/users`}>Open Identity & access</a>
      <p>
        {hasAccount
          ? "Login account linked. Roles below control access to this workspace."
          : "No login account is linked to this employee."}
      </p>
      {error && <p role="alert">{error}</p>}
      {assignments.map((a) => (
        <div className="row" key={String(a.id)}>
          <div className="row-main">
            <strong>{businessRoleLabel(String(a.role))}</strong>
            <p className="muted">
              {String(a.scope_kind)} · {String(a.scope_node_id ?? a.company_id)}
            </p>
          </div>
          <StatusBadge
            status={String(
              a.access_status ?? (a.revoked_at ? "revoked" : "active"),
            )}
          />
          {canManage && !a.revoked_at && a.role !== "holding_owner" && (
            <button
              className="button secondary"
              disabled={pending}
              onClick={() => void send("revoke", { assignmentId: a.id })}
            >
              Revoke role
            </button>
          )}
        </div>
      ))}
      {canManage && (
        <details>
          <summary>{hasAccount ? "Grant role" : "Invite to ERPX"}</summary>
          <form
            className="business-form"
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
            <label>
              System role
              <select
                name="role"
                value={role}
                onChange={(e) => setRole(e.target.value)}
              >
                {Object.keys(businessRoles).map((role) => (
                  <option key={role} value={role}>
                    {businessRoleLabel(role)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Access scope
              <select
                name="scope"
                key={role === "it_admin" ? "company" : "optional"}
                disabled={role === "it_admin"}
                defaultValue={role === "it_admin" ? "holding" : "branch"}
              >
                <option value="branch">Employee’s branch</option>
                <option value="department">Employee’s department</option>
                <option value="holding">All branches in this company</option>
              </select>
            </label>
            <label>
              Access expires
              <input name="expiresAt" type="date" required />
            </label>
            <button className="button" disabled={pending}>
              {hasAccount ? "Grant role" : "Prepare invitation"}
            </button>
          </form>
        </details>
      )}
      {notice && <p role="status">{notice}</p>}
      {canRevokeCompany &&
        userId &&
        assignments.some((a) => a.access_status === "active") && (
          <RevokeCompanyAccess base={base} userId={userId} />
        )}
    </section>
  );
}
