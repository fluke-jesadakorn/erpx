import { readFileSync } from "node:fs";
import { posix, resolve } from "node:path";

export const ownerWorkspaceDirectories = ["packages", "modules", "products", "platform"] as const;

const ownerWorkspace = /^(packages\/[^/]+|modules\/[^/]+\/(?:ui|server|iam-ui)|products\/[^/]+\/ui|platform\/(?:(?:shared|runtime)\/(?:ui|server)|(?!(?:shared|runtime|composition)(?:\/|$))[^/]+))(?=\/|$)/;

export function ownerWorkspacePath(file: string): string | undefined {
  return ownerWorkspace.exec(file)?.[1];
}

export function workspaceManifests(root: string) {
  const config = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8")) as { workspaces: string[] };
  const manifests = new Map<string, string>();
  for (const workspace of config.workspaces) {
    for (const file of new Bun.Glob(posix.join(workspace, "package.json")).scanSync({ cwd: root })) {
      if (!ownerWorkspacePath(file)) continue;
      const { name } = JSON.parse(readFileSync(resolve(root, file), "utf8")) as { name: string };
      if (manifests.has(name)) throw new Error("WORKSPACE_PACKAGE_IDENTITY_DUPLICATE:" + name);
      manifests.set(name, posix.dirname(file));
    }
  }
  return manifests;
}

export function workspaceSpecifierDirectory(name: string): string {
  if (name === "identity-access-iam-ui") return "modules/identity-access/iam-ui";
  const platform = /^platform-(?:(shared|runtime)-(ui|server)|(ui|workplace|front|contracts|native-sdk|catalogue))$/.exec(name);
  if (platform) return platform[1] ? ["platform", platform[1], platform[2]].join("/") : "platform/" + platform[3];
  const product = /^product-(.+)-ui$/.exec(name);
  if (product) return "products/" + product[1] + "/ui";
  const owner = /^(.+)-(ui|server)$/.exec(name);
  if (owner) return ["modules", owner[1], owner[2]].join("/");
  return "packages/" + name;
}
