import { relative, resolve } from "node:path";
import { repositoryRoot, testLanes, type Selection } from "./catalogue";
import { parseBrowserCaseOrder, type BrowserCaseOrder } from "./browser-execution-plan";

export type TestArguments = Selection & { command: string; list: boolean; json: boolean; check: boolean; base?: string; head?: string; caseOrder?: BrowserCaseOrder; forwarded: string[] };
const commands = new Set(["tests", ...testLanes, "impact"]);
export function parseTestArguments(args: string[], root = repositoryRoot): TestArguments {
  const input = [...args];
  const command = commands.has(input[0]) ? input.shift()! : "tests";
  const result: TestArguments = { command, ...(testLanes.includes(command as typeof testLanes[number]) ? { lane: command as typeof testLanes[number] } : {}), modules: [], suiteIds: [], files: [], list: false, json: false, check: false, forwarded: [] };
  const value = (index: number, flag: string): [string, number] => {
    const inline = input[index].slice(flag.length + 1), next = input[index + 1];
    if (input[index].startsWith(flag + "=")) {
      if (!inline || inline.split(",").some(item => !item)) throw new Error(`TEST_OPTION_VALUE_REQUIRED:${flag}`);
      return [inline, index];
    }
    if (!next || next.startsWith("-")) throw new Error(`TEST_OPTION_VALUE_REQUIRED:${flag}`);
    return [next, index + 1];
  };
  for (let index = 0; index < input.length; index++) {
    const arg = input[index];
    if (arg === "--") continue;
    if (["--list", "--json", "--check"].includes(arg)) { result[arg.slice(2) as "list" | "json" | "check"] = true; continue; }
    const option = ["--module", "--suite", "--base", "--head", "--case-order"].find(flag => arg === flag || arg.startsWith(flag + "="));
    if (option) {
      const [selected, next] = value(index, option); index = next;
      if (option === "--module") result.modules!.push(...selected.split(","));
      if (option === "--suite") result.suiteIds!.push(...selected.split(","));
      if (option === "--base") result.base = selected;
      if (option === "--head") result.head = selected;
      if (option === "--case-order") {
        if (result.caseOrder) throw new Error("DUPLICATE_BROWSER_CASE_ORDER");
        result.caseOrder = parseBrowserCaseOrder(selected);
      }
      continue;
    }
    if (/\.(?:test|spec)\.[cm]?[jt]sx?$/.test(arg)) {
      result.files!.push(relative(root, resolve(root, arg)).replaceAll("\\", "/"));
      continue;
    }
    if (!arg.startsWith("-") && (arg.startsWith("tests/") || arg.includes(".test.") || arg.includes(".spec."))) throw new Error(`INVALID_TEST_FILE:${arg}`);
    if (arg.startsWith("--module") || arg.startsWith("--suite") || arg.startsWith("--base") || arg.startsWith("--head") || arg.startsWith("--case-order")) throw new Error(`UNKNOWN_TEST_OPTION:${arg}`);
    result.forwarded.push(arg);
  }
  if (command === "impact" && !result.base) throw new Error("IMPACT_BASE_REQUIRED:use --base=<commit>");
  if (command !== "impact" && (result.base || result.head)) throw new Error("GIT_REFERENCE_ONLY_FOR_IMPACT");
  if (command === "impact" && (result.modules!.length || result.suiteIds!.length || result.files!.length)) throw new Error("IMPACT_SELECTION_IS_COMPUTED_FROM_GIT");
  if (result.caseOrder && !["impact", "e2e", "journeys", "uat"].includes(command)) throw new Error("BROWSER_CASE_ORDER_ONLY_FOR_BROWSER_LANES");
  return result;
}
