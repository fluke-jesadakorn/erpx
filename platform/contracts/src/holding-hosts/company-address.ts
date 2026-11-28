/** Public workspace segments in Holding → Company → Branch → Department order. */
export type CompanyAddress = {
  company: string;
  workspace?: string;
  branch?: string;
  department?: string;
  destination: string;
  tail: string[];
};

const slug = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;

export function parseCompanyAddress(
  segments: readonly string[],
  isProduct: (segment: string) => boolean,
  isReserved: (segment: string) => boolean,
): CompanyAddress | null {
  const [company] = segments;
  const isWorkspace = (value: string) => slug.test(value) && !isReserved(value);
  if (!company || !isWorkspace(company)) return null;

  const product = segments.findIndex((value, index) => index > 0 && isProduct(value));
  const index = product < 0 ? segments.length - 1 : product;
  const destination = segments[index];
  if (index < 1 || index > 3 || !destination) return null;
  if (product < 0 && !isWorkspace(destination)) return null;

  const scope = segments.slice(1, index);
  if (!scope.every(isWorkspace)) return null;
  const tail = segments.slice(index + 1);
  if (scope.length === 1) return { company, workspace: scope[0], destination, tail };
  if (scope.length === 2) return { company, branch: scope[0], department: scope[1], destination, tail };
  return { company, destination, tail };
}

export function companyAddressPath(
  company: string,
  branch: string | undefined,
  department: string | undefined,
  destination: string,
  tail = "",
): string {
  return "/" + [company, branch, department, destination, tail].filter(Boolean).join("/");
}
