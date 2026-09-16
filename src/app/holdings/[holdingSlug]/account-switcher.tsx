"use client";

import { workFetch } from "@/lib/work-fetch";
import { RoleSelect } from "@/components/workplace/role-select";
import { PersonAvatar } from "@/components/ui/person-avatar";
import { useHydrated } from "@/components/ui/use-hydrated";

import { useEffect, useRef, useState } from "react";
import {
  Check,
  ChevronDown,
  LogIn,
  LogOut,
  Plus,
  UserRound,
} from "lucide-react";
import styles from "@/components/workplace/account.module.css";
import Link from "next/link";
import { authClient } from "@/lib/auth-client";
import { Appearance } from "@/components/workplace/workplace-controls";

type DeviceSession = {
  session: { token: string; userId: string };
  user: { id: string; name: string; email: string; image?: string | null };
};

type Props = {
  assignments?: import("@/server/authorization").WorkAssignment[];
  preferredAssignmentId?: string | null;
  contextRevision?: number;
  workspaceBase: string;
  activeUser: { id: string; name: string; email: string };
  theme: string;
};

function accountSwitchDestination(workspaceBase: string) {
  const destination = new URL("/", window.location.origin);
  destination.searchParams.set("account", "switch");
  destination.searchParams.set("from", `${workspaceBase}/home`);
  return destination;
}

export function AccountSwitcher({ workspaceBase, activeUser, theme, assignments = [], preferredAssignmentId = null, contextRevision = 0 }: Props) {
  const ready = useHydrated();
  const root = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [adding, setAdding] = useState(false);
  const [pending, setPending] = useState("");
  const [error, setError] = useState("");
  const [sessions, setSessions] = useState<DeviceSession[]>([]);

  async function loadAccounts() {
    const result = await authClient.multiSession.listDeviceSessions();
    if (result.data) setSessions(result.data as DeviceSession[]);
  }

  useEffect(() => {
    function close(event: MouseEvent) {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    }
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, []);

  useEffect(() => {
    if (!open) return;
    const previous = document.activeElement as HTMLElement | null;
    const menu = root.current?.querySelector<HTMLElement>('[role="dialog"]');
    const focusable = () =>
      Array.from(
        menu?.querySelectorAll<HTMLElement>(
          "button:not(:disabled),input:not(:disabled),a[href],select",
        ) ?? [],
      ).filter((element) => element.getClientRects().length);
    focusable()[0]?.focus();
    function keyboard(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        setOpen(false);
      }
      if (event.key === "Tab") {
        const items = focusable(),
          first = items[0],
          last = items.at(-1);
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault();
          last?.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first?.focus();
        }
      }
    }
    document.addEventListener("keydown", keyboard);
    return () => {
      document.removeEventListener("keydown", keyboard);
      previous?.focus();
    };
  }, [open, adding]);

  async function chooseAccount(session: DeviceSession) {
    if (session.user.id === activeUser.id) return;
    setPending(`account:${session.user.id}`);
    setError("");
    const result = await authClient.multiSession.setActive({
      sessionToken: session.session.token,
    });
    if (result.error) {
      setError(result.error.message ?? "Account could not be selected.");
      setPending("");
      return;
    }
    // The selected account may belong to a different Holding. Resolve the
    // first workspace it is actually authorized to use instead of reloading
    // the current Holding, which would be intentionally hidden as a 404.
    window.location.replace(accountSwitchDestination(workspaceBase));
  }

  async function addAccount(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setPending("add");
    setError("");
    const form = new FormData(event.currentTarget);
    const result = await authClient.signIn.email({
      email: String(form.get("email")),
      password: String(form.get("password")),
      // Root resolves the newly active account's first authorized workspace;
      // the current Holding is not necessarily available to that account.
      callbackURL: "/",
    });
    if (result.error) {
      setError(result.error.message ?? "Account could not be added.");
      setPending("");
      return;
    }
    window.location.replace(accountSwitchDestination(workspaceBase));
  }

  async function chooseRole(assignmentId: string) {
    if(document.querySelector<HTMLElement>("[data-work-dirty=true]") && !window.confirm("Change role and discard unsaved changes?"))return;
    setPending("role"); setError("");
    try {
      const response = await workFetch(`/api/v1${workspaceBase}/assignments/active`, {
        method: "POST", headers: { "content-type": "application/json", "x-erpx-context-company": assignments.find(option=>option.id===assignmentId)?.companyId??"" },
        body: JSON.stringify({ assignmentId, revision: contextRevision }),
      });
      if (!response.ok) throw new Error("Role could not be selected. Refresh to check your current access.");
      const {data}=await response.json();
      const channel=new BroadcastChannel("erpx-work-context");channel.postMessage({changed:true});channel.close();
      window.location.assign(data.href);
    } catch (error) { setError(error instanceof Error ? error.message : "Role could not be selected."); setPending(""); }
  }

  async function signOut() {
    setPending("logout");
    await authClient.signOut();
    window.location.replace(new URL("/login", window.location.origin));
  }

  const accounts = sessions.some((item) => item.user.id === activeUser.id)
    ? sessions
    : [
        { session: { token: "", userId: activeUser.id }, user: activeUser },
        ...sessions,
      ];

  return (
    <div className={`account-switcher ${styles.skin}`} ref={root}>
      <button
        className="account-trigger"
        disabled={!ready}
        aria-label={`Account menu for ${activeUser.name}, ${activeUser.email}`}
        title={`${activeUser.name} · ${activeUser.email}`}
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => {
          const next = !open;
          setOpen(next);
          setAdding(false);
          setError("");
          if (next) void loadAccounts();
        }}
      >
        <PersonAvatar name={activeUser.name} small />
        <span className="user-copy">
          <strong>{activeUser.name}</strong>
          <span className="active-work-role" title={assignments.find(a=>a.id===preferredAssignmentId)?.label}>{assignments.find(a=>a.id===preferredAssignmentId)?.label ?? activeUser.email}</span>
        </span>
        <ChevronDown size={15} />
      </button>
      {open && (
        <div
          className="account-menu"
          role="dialog"
          aria-label="Account switcher"
        >
          <div className="account-menu-head">
            <div>
              <span className="eyebrow">Signed in accounts</span>
              <strong>Account & work role</strong>
            </div>
            <span className="session-count">{accounts.length}/5</span>
          </div>
          {!adding && (
            <>
              <div className="account-list">
                {accounts.map((item) => {
                  const active = item.user.id === activeUser.id;
                  return (
                    <button
                      type="button"
                      className={`account-option ${active ? "active" : ""}`}
                      key={item.user.id}
                      disabled={Boolean(pending)}
                      onClick={() => void chooseAccount(item)}
                    >
                      <PersonAvatar name={item.user.name} />
                      <span>
                        <strong>{item.user.name}</strong>
                        <small>{item.user.email}</small>
                      </span>
                      {active ? (
                        <Check size={17} />
                      ) : pending === `account:${item.user.id}` ? (
                        <span className="mini-spinner" />
                      ) : (
                        <LogIn size={16} />
                      )}
                    </button>
                  );
                })}
              </div>
              <Link className="button ghost" href={`${workspaceBase}/iam/my-access`} onClick={() => setOpen(false)}>My access & security</Link>
              {workspaceBase !== "/platform" && <Link className="button ghost" href={`${workspaceBase}/people?member=${activeUser.id}`} onClick={() => setOpen(false)}>My profile</Link>}
              {assignments.length > 0 && <section className="role-section">
                <RoleSelect assignments={assignments} value={preferredAssignmentId ?? ""} disabled={Boolean(pending)} onChange={value => void chooseRole(value)} />
                <p className="role-hint">Only this role is used for actions and approvals. Self-approval rules still apply.</p>
              </section>}
              <div className="appearance-row"><span>Appearance</span><Appearance initialTheme={theme} compact /></div>
              <div className="account-actions">
                <button
                  type="button"
                  onClick={() => {
                    setAdding(true);
                    setError("");
                  }}
                  disabled={accounts.length >= 5}
                >
                  <Plus size={16} />
                  Add another account
                </button>
                <button type="button" onClick={() => void signOut()}>
                  <LogOut size={16} />
                  Sign out all
                </button>
              </div>
            </>
          )}
          {adding && (
            <form className="add-account-form" onSubmit={addAccount}>
              <div className="add-account-title">
                <span>
                  <UserRound size={18} />
                </span>
                <div>
                  <strong>Add an existing account</strong>
                  <small>
                    Verify its credentials once to keep it available on this
                    device.
                  </small>
                </div>
              </div>
              <label>
                Work email
                <input
                  name="email"
                  type="email"
                  autoComplete="username"
                  required
                  autoFocus
                />
              </label>
              <label>
                Password
                <input
                  name="password"
                  type="password"
                  autoComplete="current-password"
                  minLength={12}
                  required
                />
              </label>
              <div className="add-account-actions">
                <button
                  type="button"
                  className="button secondary"
                  onClick={() => setAdding(false)}
                >
                  Back
                </button>
                <button className="button" disabled={pending === "add"}>
                  {pending === "add" ? "Adding…" : "Add account"}
                </button>
              </div>
            </form>
          )}
          {error && (
            <div className="account-error" role="alert">
              {error}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
