import type { QuickAccessItem } from "@/lib/work_projection/quick-access";

export type AccountQuickAccessArea = "my-access" | "security" | "work-role";

export function accountQuickAccessItems(
  area: AccountQuickAccessArea,
  options: { hasWorkspace: boolean; hasRoles?: boolean },
): QuickAccessItem[] {
  if (area === "my-access") return [
    { id: "roles", title: "Roles & permissions", description: "Review your current assignments", href: "/my-access?widget=account-roles", icon: "account", tone: "blue" },
    { id: "security", title: "Account security", description: "Authenticator and sessions", href: "/security", icon: "security", tone: "violet" },
    ...(options.hasWorkspace ? [{ id: "workspace", title: "Open workspace", href: "/", icon: "workspace", tone: "brand" as const }] : []),
  ];
  if (area === "security") return [
    { id: "authenticator", title: "Authenticator & sessions", description: "Manage sign-in protection", href: "/security?widget=account-security-controls", icon: "security", tone: "violet" },
    { id: "access", title: "My access", description: "Roles and permissions", href: "/my-access", icon: "account", tone: "blue" },
    ...(options.hasWorkspace ? [{ id: "workspace", title: "Open workspace", href: "/", icon: "workspace", tone: "brand" as const }] : []),
  ];
  return [
    ...(options.hasRoles ? [{ id: "choose-role", title: "Choose a role", description: "Select an active work assignment", href: "#role-picker", icon: "account", tone: "blue" as const }] : []),
    ...(options.hasWorkspace ? [{ id: "workspaces", title: "Available workspaces", href: "#available-workspaces", icon: "workspace", tone: "brand" as const }] : []),
    { id: "access", title: "My access", description: "Review your assignments", href: "/my-access", icon: "account", tone: "teal" },
    { id: "security", title: "Account security", href: "/security", icon: "security", tone: "violet" },
  ];
}
