export function agentSourceAllowed(path: string, allowedSources: readonly string[]) {
  return allowedSources.some(scope => scope === "/" || path === scope ||
    !scope.includes("?") && (path.startsWith(`${scope}/`) || path.startsWith(`${scope}?`)));
}
