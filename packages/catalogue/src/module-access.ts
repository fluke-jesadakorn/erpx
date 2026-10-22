export function hasModuleAccess(
  allowedModules: readonly string[],
  moduleId: string,
) {
  return allowedModules.includes(moduleId);
}
