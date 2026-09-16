import Link from "next/link";
import type { PermissionActor } from "@/lib/iam";
import {
  workplaceNavigation,
  type NavigationScopeKind,
} from "@/lib/workplace-navigation";
import styles from "@/components/workplace/pages.module.css";
export function PeopleNavigation({
  base,
  actor,
  active,
  scopeKind,
  canPayroll = [
    "holding_owner",
    "hr_admin",
    "people_manager",
    "payroll_preparer",
    "payroll_approver",
    "auditor",
  ].includes(actor.role) &&
    !base.includes("/branches/") &&
    !base.includes("/departments/"),
}: {
  base: string;
  actor: PermissionActor;
  active: "directory" | "organization" | "access" | "payroll";
  scopeKind: NavigationScopeKind;
  canPayroll?: boolean;
}) {
  const people = workplaceNavigation({
    workspaceBase: base,
    scopeKind,
    role: actor.role,
    permissions: actor.permissions,
  }).groups.find((group) => group.id === "people")!;
  const ids: Record<string, typeof active> = {
    "people-organization": "organization",
    "people-payroll": "payroll",
    "people-access": "access",
  };
  const links = [
    ["directory", people.href!, "Directory"],
    ...people.items
      .filter((item) => canPayroll || item.id !== "people-payroll")
      .map((item) => [ids[item.id], item.href, item.label]),
  ];
  return (
    <nav className={styles.tabs} aria-label="People views">
      {links.map(([id, href, label]) => (
        <Link
          key={id}
          href={href}
          aria-current={active === id ? "page" : undefined}
        >
          {label}
        </Link>
      ))}
    </nav>
  );
}
