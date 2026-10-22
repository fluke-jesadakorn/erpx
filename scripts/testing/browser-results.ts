export type BrowserAnnotation = { type: string; description?: string };
export type BrowserCase = { file: string; fileTitle?: string; title: string; project: "desktop" | "mobile"; expectedStatus: string; annotations?: BrowserAnnotation[] };
type Suite = { title?: string; suites?: Suite[]; specs?: { file: string; title: string; tests: { projectName: string; expectedStatus: string; status?: string; results?: { status: string }[]; annotations?: BrowserAnnotation[] }[] }[] };

function canonicalBrowserFile(file: string): string {
  const normalized = file.replaceAll("\\", "/");
  const index = normalized.lastIndexOf("tests/");
  if (index >= 0) return normalized.slice(index);
  return normalized.startsWith("/") ? normalized : `tests/${normalized}`;
}

export function sameBrowserFileIdentity(actual: string, expected: string): boolean {
  return canonicalBrowserFile(actual) === canonicalBrowserFile(expected);
}

export function browserCases(report: { suites: Suite[] }): BrowserCase[] {
  const cases: BrowserCase[] = [];
  function visit(suite: Suite, parents: string[], fileTitle?: string) {
    if (suite.title && /\.spec\.[cm]?[jt]sx?$/.test(suite.title)) fileTitle = suite.title;
    const names = suite.title && !/\.spec\.[cm]?[jt]sx?$/.test(suite.title) ? [...parents, suite.title] : parents;
    for (const spec of suite.specs ?? []) for (const test of spec.tests) {
      if (test.projectName !== "desktop" && test.projectName !== "mobile") throw new Error("UNKNOWN_BROWSER_PROJECT");
      cases.push({ file: spec.file, ...(fileTitle ? { fileTitle } : {}), title: [...names, spec.title].join(" "), project: test.projectName, expectedStatus: test.expectedStatus, ...(test.annotations ? { annotations: test.annotations } : {}) });
    }
    for (const child of suite.suites ?? []) visit(child, names, fileTitle);
  }
  for (const suite of report.suites) visit(suite, []);
  return cases;
}

/** An exit code alone can hide skip, expected failure, or an empty grep. */
export function assertBrowserCasePassed(report: { suites: Suite[] }, wanted: BrowserCase) {
  const cases = browserCases(report);
  if (cases.length !== 1 || !sameBrowserFileIdentity(cases[0].file, wanted.file) || cases[0].title !== wanted.title || cases[0].project !== wanted.project)
    throw new Error("EXACT_BROWSER_CASE_EXECUTION_REQUIRED");
  let passed = false;
  function visit(suite: Suite) {
    for (const spec of suite.specs ?? []) for (const test of spec.tests) {
      passed = test.expectedStatus === "passed" && test.status === "expected" && test.results?.length === 1 && test.results[0].status === "passed";
    }
    for (const child of suite.suites ?? []) visit(child);
  }
  for (const suite of report.suites) visit(suite);
  if (!passed) throw new Error("REQUIRED_BROWSER_CASE_DID_NOT_PASS");
}

export function exactTitlePattern(title: string, project?: string, fileTitle?: string) {
  if (Boolean(project) !== Boolean(fileTitle)) throw new Error("EXACT_BROWSER_GREP_IDENTITY_REQUIRED");
  const full = project ? `${project} ${fileTitle} ${title}` : title;
  const escaped = full.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  // Playwright's grep includes the root/project/file titles and optional tags.
  return project ? `^\\s*${escaped}(?: @[^ ]+)*$` : `^${escaped}$`;
}

export function browserProjects(projects: ("desktop" | "mobile")[], reason: string) {
  if (!projects.length || new Set(projects).size !== projects.length || !reason.trim()) throw new Error("INVALID_BROWSER_PROJECT_REQUIREMENT");
  return { annotation: { type: "erpx-browser-projects", description: JSON.stringify({ projects, reason }) } };
}

/** A declared device restriction is inspectable coverage, never an accepted runtime skip. */
export function browserCaseProjectRequirement(wanted: BrowserCase): { required: boolean; reason?: string } {
  const annotations = wanted.annotations?.filter(annotation => annotation.type === "erpx-browser-projects") ?? [];
  if (!annotations.length) return { required: true };
  if (annotations.length !== 1 || !annotations[0].description) throw new Error("INVALID_BROWSER_PROJECT_REQUIREMENT");
  let value: unknown;
  try { value = JSON.parse(annotations[0].description); } catch { throw new Error("INVALID_BROWSER_PROJECT_REQUIREMENT"); }
  const policy = value as { projects?: unknown; reason?: unknown };
  if (!policy || !Array.isArray(policy.projects) || !policy.projects.length || policy.projects.some(project => !["desktop", "mobile"].includes(project)) || new Set(policy.projects).size !== policy.projects.length || typeof policy.reason !== "string" || !policy.reason.trim()) throw new Error("INVALID_BROWSER_PROJECT_REQUIREMENT");
  return { required: policy.projects.includes(wanted.project), reason: policy.reason };
}
