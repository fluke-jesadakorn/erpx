/** Request-private product discovery. These maps never replace an acting Work
 * context or become initial Client props. */
export type ProductModuleAccess = {
  modulesByAssignment: ReadonlyMap<string, readonly string[]>;
  presentationModulesByAssignment: ReadonlyMap<string, readonly string[]>;
  permissionsByAssignment: ReadonlyMap<string, readonly string[]>;
  beta: boolean;
};
