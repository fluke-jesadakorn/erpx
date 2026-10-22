import { expect, test } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { createEngineeringBrowserContext } from "../../support/browser/engineering-browser-evidence";
import { createWorkflowUiObserver } from "../../support/browser/workflow-ui-observation";

const formsOrigin = "https://forms-dev-erpx.jesadakorn.com";
const compiledOrigin = "http://127.0.0.1:3104";

type BrowserState = {
  run: string;
  prefix: string;
  origin: string;
  port: number;
  ready: boolean;
  formsHosts: boolean;
  productHosts: boolean;
  executiveWorkHosts: boolean;
  fixture?: { engineeringWorkplacePlatform?: unknown };
};

type WorkplacePlatformGuide = {
  run: string;
  marketing: {
    formId: string;
    formVersion: number;
    publicationId: string;
  };
};

test("isolated anonymous Forms host submits the published version and replays exact consent once", async ({ browser }, testInfo) => {
  test.setTimeout(120_000);

  const run = process.env.ERPX_DEMO_RUN;
  if (!run || !/^[a-f0-9]{12}$/.test(run)) throw new Error("EXACT_DISPOSABLE_FORMS_RUN_REQUIRED");
  if (process.env.ERPX_E2E_BASE_URL !== formsOrigin) throw new Error("EXACT_FORMS_HOST_ORIGIN_REQUIRED");

  const directory = resolve(".runtime/browser", run);
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as BrowserState;
  if (state.run !== run || state.prefix !== `erpx_browser_${run}` || state.origin !== compiledOrigin || state.port !== 3104
    || state.ready !== true || state.formsHosts !== true || state.productHosts !== false || state.executiveWorkHosts !== false
    || !state.fixture?.engineeringWorkplacePlatform) {
    throw new Error("EXACT_FORMS_ENGINEERING_COHORT_REQUIRED");
  }

  const guide = JSON.parse(await readFile(resolve(directory, "workplace-platform-guide.json"), "utf8")) as WorkplacePlatformGuide;
  if (guide.run !== run || !guide.marketing?.formId || !Number.isSafeInteger(guide.marketing.formVersion)
    || !guide.marketing.publicationId) {
    throw new Error("EXACT_FORMS_WORKPLACE_GUIDE_REQUIRED");
  }

  const { formId, formVersion, publicationId } = guide.marketing;
  const visitorContext = await createEngineeringBrowserContext(browser, testInfo, {
    baseURL: formsOrigin,
    viewport: testInfo.project.name === "mobile" ? { width: 390, height: 844 } : { width: 1440, height: 900 },
    deviceScaleFactor: 1,
  });
  try {
    const visitor = await visitorContext.newPage();
    const source = `Declared fictional Forms visitor ${run}`;
    const pageUrl = new URL(`/forms/simulation/${encodeURIComponent(formId)}?utm_source=${encodeURIComponent(source)}`, formsOrigin);
    const pageResponse = await visitor.goto(pageUrl.href, { waitUntil: "domcontentloaded" });
    expect(pageResponse?.status()).toBe(200);
    expect(visitor.url()).toBe(pageUrl.href);

    const publicFormUrl = new URL(`/api/public/marketing/simulation/${encodeURIComponent(formId)}`, formsOrigin).href;
    const publicFormResponse = visitor.waitForResponse(response => response.url() === publicFormUrl && response.request().method() === "GET");
    const publicFormFetch = visitor.evaluate(async url => {
      const response = await fetch(url, { cache: "no-store" });
      return { status: response.status, data: (await response.json()).data };
    }, publicFormUrl);
    const [publicFormResponseResult, publicFormResult] = await Promise.all([publicFormResponse, publicFormFetch]);
    expect(publicFormResponseResult.status()).toBe(200);
    expect(publicFormResult.status).toBe(200);
    const publicForm = publicFormResult.data;
    expect(publicForm).toMatchObject({ id: formId, version: formVersion, consentRequired: true });
    expect(publicForm.presentation).toMatchObject({
      content: { headline: "Fictional workplace equipment demonstration" },
      media: { id: publicationId, altText: "Declared ERPX icon for the fictional equipment inquiry" },
    });

    await expect(visitor.getByRole("heading", { name: "Fictional workplace equipment inquiry", exact: true })).toBeVisible();
    await expect(visitor.getByRole("region", { name: "Approved form content" })).toContainText(
      "A fictional equipment team can submit an inquiry for human review.",
    );
    const image = visitor.getByRole("img", { name: "Declared ERPX icon for the fictional equipment inquiry", exact: true });
    await expect(image).toBeVisible();
    const imageSource = await image.getAttribute("src");
    if (!imageSource) throw new Error("FORMS_APPROVED_MEDIA_SOURCE_REQUIRED");
    expect(new URL(imageSource, visitor.url()).href).toBe(
      new URL(`/api/public/holdings/simulation/media-publications/${encodeURIComponent(publicationId)}/content`, formsOrigin).href,
    );
    await expect.poll(() => image.evaluate(node => (node as HTMLImageElement).naturalWidth)).toBeGreaterThan(0);

    const contactName = `Fictional Forms visitor ${run}`;
    const contactDetail = `forms-visitor-${run}@simulation.test`;
    const brief = `Fictional Forms-host inquiry for isolated run ${run}; human review only.`;
    await visitor.getByLabel("Name", { exact: true }).fill(contactName);
    await visitor.getByLabel("How can we contact you?", { exact: true }).fill(contactDetail);
    await visitor.getByLabel("How can we help?", { exact: true }).fill(brief);
    await visitor.locator('[name="consent"]').check();

    const postUrl = new URL(`/api/public/marketing/simulation/${encodeURIComponent(formId)}`, formsOrigin).href;
    const responsePromise = visitor.waitForResponse(response => response.url() === postUrl && response.request().method() === "POST");
    await visitor.getByRole("button", { name: "Send request", exact: true }).click();
    const response = await responsePromise;
    expect(response.status()).toBe(201);
    expect(response.headers()["cache-control"]).toBe("no-store");

    const request = response.request();
    expect(request.headers().origin).toBe(formsOrigin);
    const idempotencyKey = request.headers()["idempotency-key"];
    expect(idempotencyKey).toMatch(/^[A-Za-z0-9:_.-]{8,180}$/);
    const capturedBody = request.postData();
    if (capturedBody === null) throw new Error("FORMS_SUBMISSION_BODY_REQUIRED");
    expect(JSON.parse(capturedBody)).toEqual({
      formVersion,
      contactName,
      contactDetail,
      brief,
      consentAccepted: true,
      source,
    });

    const receipt = (await response.json()).data;
    expect(receipt).toMatchObject({ id: expect.any(String), status: "pending" });
    await expect(visitor.getByRole("heading", { name: "Thank you", exact: true })).toBeVisible();
    await expect(visitor.getByRole("status")).toContainText("Your message is awaiting review.");

    const replayRequestPromise = visitor.waitForRequest(request => request.url() === postUrl && request.method() === "POST");
    const replayResponsePromise = visitor.waitForResponse(response => response.url() === postUrl && response.request().method() === "POST");
    const replayFetch = visitor.evaluate(async ({ url, body, key }) => {
      const response = await fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": key },
        body,
      });
      return { status: response.status };
    }, { url: postUrl, body: capturedBody, key: idempotencyKey });
    const [replayRequest, replayResponse, replayResult] = await Promise.all([
      replayRequestPromise,
      replayResponsePromise,
      replayFetch,
    ]);
    expect(replayResult.status).toBe(201);
    expect(replayResponse.status()).toBe(201);
    expect(replayRequest.postData()).toBe(capturedBody);
    expect(replayRequest.headers().origin).toBe(formsOrigin);
    expect(replayRequest.headers()["idempotency-key"]).toBe(idempotencyKey);
    expect((await replayResponse.json()).data).toEqual(receipt);

    const observer = await createWorkflowUiObserver(visitor, testInfo, {
      routeSource: "src/app/forms/[holdingSlug]/[formId]/page.tsx",
      targetSource: "src/components/marketing/public-marketing-form.tsx",
      checks: [
        "published form and reviewed content render without authentication",
        "required consent is submitted with the published form version",
        "same-key replay returns the original receipt",
      ],
      ownerContext: "Anonymous public visitor; no ERPX account or authenticated session",
      authorityBranch: "published public marketing form",
      authentication: "anonymous-public-visitor",
    });
    await observer.observe("forms-published-consent-replay");
    await observer.stable();
  } finally {
    await visitorContext.close();
  }
});
