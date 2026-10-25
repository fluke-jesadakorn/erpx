import { copyFile, mkdir, readFile } from "node:fs/promises";
import { randomBytes } from "node:crypto";
import { join, resolve } from "node:path";
import { runBrowserSuites } from "./browser-runner";
import { loadCatalogue, repositoryRoot, selectSuites } from "./catalogue";
import { investorCases } from "./investor-selection";
import { collectInvestorReport, generateInvestorReport } from "./investor-report";

const args = process.argv.slice(2);
const mode = args.shift();
if (mode === "report") {
  const from = args.find(arg => arg.startsWith("--from="))?.slice(7);
  if (!from || args.some(arg => !arg.startsWith("--from="))) throw new Error("Use investor:report --from=<run-directory>");
  const complete = await generateInvestorReport(resolve(from));
  console.log(JSON.stringify({ directory: resolve(from), complete }));
  if (!complete) process.exitCode = 1;
} else if (mode === "capture") {
  if (args.length) throw new Error("Investor capture uses the complete curated selection; no additional arguments are accepted.");
  if (Bun.version !== "1.2.17") throw new Error("PINNED_BUN_REQUIRED");
  const directory = join(repositoryRoot, ".scratch/dev/investor", new Date().toISOString().replace(/[:.]/g, "-") + "-" + randomBytes(3).toString("hex"));
  await mkdir(directory, { recursive: true, mode: 0o700 });
  await copyFile(join(repositoryRoot, "public/brand/erpx-lockup.svg"), join(directory, "brand.svg"));
  let evidence: string | undefined;
  let failed = false;
  try {
    // Match the standard test command's restricted administrator configuration.
    if (!process.env.ERPX_TEST_ADMIN_URL) for (const name of [".env.test.local", ".env.test", ".env.local"]) {
      let value: string | undefined;
      try { value = (await readFile(join(repositoryRoot, name), "utf8")).match(/^\s*(?:export\s+)?ERPX_TEST_ADMIN_URL\s*=\s*(.*?)\s*$/m)?.[1]; }
      catch (error) { if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error; }
      if (value) { process.env.ERPX_TEST_ADMIN_URL = value.replace(/^(["'])(.*)\1$/, "$2"); break; }
    }
    if (!process.env.ERPX_TEST_ADMIN_URL) throw new Error("ISOLATED_TEST_ADMIN_REQUIRED");
    const suites = selectSuites(loadCatalogue(), { suiteIds: [...new Set(investorCases.map(item => item.suite))] });
    console.log(`Investor capture: ${investorCases.length} journey/device cases; output ${directory}`);
    await runBrowserSuites(suites, { caseSelection: investorCases, investorCapture: true, port: 3104, onInvestorEvidence: value => { evidence = value; } });
  } catch {
    failed = true;
    console.error("Investor capture did not complete. Inspect the private browser evidence directory; the presentation report will be marked incomplete.");
  } finally {
    await collectInvestorReport(evidence, directory);
    const complete = await generateInvestorReport(directory);
    console.log(JSON.stringify({ directory, complete, evidenceDirectory: evidence }));
    if (failed || !complete) process.exitCode = 1;
  }
} else throw new Error("Expected capture or report.");
