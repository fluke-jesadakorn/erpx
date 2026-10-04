import { chromium } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { gzipSync, brotliCompressSync } from "node:zlib";
const port = Number(process.env.ERPX_UI_BENCHMARK_PORT ?? "3104");
if (![3104, 3106].includes(port)) throw new Error("Use an isolated benchmark port.");
const origin = `http://127.0.0.1:${port}`;
const email = process.env.ERPX_E2E_ADMIN_EMAIL;
const password = process.env.ERPX_E2E_ADMIN_PASSWORD;
if (!email || !password)
  throw new Error(
    "Provide existing development benchmark credentials through ERPX_E2E_ADMIN_EMAIL/PASSWORD.",
  );
const label = process.argv[2];
if (!/^[a-z-]+$/.test(label ?? "")) throw new Error("Provide a benchmark label.");
const browser = await chromium.launch();
try {
  const auth = await browser.newContext({
    baseURL: origin,
    extraHTTPHeaders: { "x-forwarded-proto": "https" },
  });
  // Authenticate through the real sign-in endpoint. The loopback process uses
  // this HTTPS public origin, as it would behind its TLS reverse proxy.
  const response = await auth.request.post("/api/auth/sign-in/email", {
    headers: { origin: `https://127.0.0.1:${port}` },
    data: { email, password },
  });
  if (!response.ok()) throw new Error(`Benchmark login returned ${response.status()}.`);
  const state = await auth.storageState();
  const routes = [
    "/login",
    "/holdings/acme/home",
    "/holdings/acme/work",
    "/holdings/acme/people",
    "/holdings/acme/finance",
    "/holdings/acme/pos",
    "/holdings/acme/pms",
  ];
  const result = {
    observationWindowMs: 6000,
    label,
    measuredAt: new Date().toISOString(),
    viewport: { width: 1440, height: 900 },
    source: "isolated production build, loopback server, cold browser contexts, development data",
    pages: [],
  };
  for (const route of routes) {
    const runs = [];
    for (let run = 0; run < 3; run++) {
      const context = await browser.newContext({
        storageState: route === "/login" ? undefined : state,
        viewport: result.viewport,
        extraHTTPHeaders: { "x-forwarded-proto": "https" },
      });
      const page = await context.newPage();
      await page.addInitScript(() => {
        window.__uiVitals = { lcp: 0, cls: 0 };
        new PerformanceObserver((list) => {
          for (const entry of list.getEntries()) window.__uiVitals.lcp = entry.startTime;
        }).observe({ type: "largest-contentful-paint", buffered: true });
        new PerformanceObserver((list) => {
          for (const entry of list.getEntries())
            if (!entry.hadRecentInput) window.__uiVitals.cls += entry.value;
        }).observe({ type: "layout-shift", buffered: true });
      });
      const cdp = await context.newCDPSession(page);
      await cdp.send("Network.enable");
      await cdp.send("Network.setCacheDisabled", { cacheDisabled: true });
      let transferBytes = 0;
      let requests = 0;
      cdp.on("Network.requestWillBeSent", () => {
        requests++;
      });
      cdp.on("Network.loadingFinished", (event) => {
        transferBytes += event.encodedDataLength;
      });
      const pending = [];
      page.on("response", (response) => {
        if (!/\.(css|js|woff2|ttf)(?:\?|$)/.test(response.url())) return;
        pending.push(
          response
            .body()
            .then((body) => ({
              url: new URL(response.url()).pathname,
              raw: body.length,
              gzip: gzipSync(body).length,
              brotli: brotliCompressSync(body).length,
            }))
            .catch(() => null),
        );
      });
      const navigation = await page.goto(origin + route, {
        waitUntil: "domcontentloaded",
        timeout: 60000,
      });
      await page.waitForTimeout(6000);
      if (navigation?.status() !== 200 || new URL(page.url()).pathname !== route)
        throw new Error(`Unexpected benchmark navigation for ${route}: ${navigation?.status()}.`);
      const vitals = await page.evaluate(() => window.__uiVitals);
      const assets = (await Promise.all(pending)).filter(Boolean);
      const byType = {};
      for (const asset of assets) {
        const kind = asset.url.endsWith(".css") ? "css" : asset.url.endsWith(".js") ? "js" : "font";
        const total = (byType[kind] ??= { requests: 0, raw: 0, gzip: 0, brotli: 0 });
        total.requests++;
        total.raw += asset.raw;
        total.gzip += asset.gzip;
        total.brotli += asset.brotli;
      }
      runs.push({
        status: navigation?.status(),
        finalPath: new URL(page.url()).pathname,
        requests,
        transferBytes,
        ...vitals,
        assets: byType,
      });
      await context.close();
    }
    result.pages.push({ route, runs });
    console.log(
      route,
      runs
        .map(
          (run) =>
            `${run.status} ${(run.transferBytes / 1024).toFixed(1)}KiB ${run.requests}req LCP:${Math.round(run.lcp)} CLS:${run.cls.toFixed(3)}`,
        )
        .join(" | "),
    );
  }
  await mkdir(".scratch/ui-refactor", { recursive: true });
  await writeFile(`.scratch/ui-refactor/${label}-pages.json`, JSON.stringify(result, null, 2));
} finally {
  await browser.close();
}
