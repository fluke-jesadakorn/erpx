import type { Page, TestInfo } from "@playwright/test";
import { describe, expect, it, vi } from "vitest";
import { captureCaseScreenshot } from "../../support/helpers/case-screenshot";
import { serializeWorkflowUiObservation } from "../../support/browser/workflow-ui-observation";

type ScreenshotOptions = NonNullable<Parameters<Page["screenshot"]>[0]>;
type CaptureOptions = NonNullable<Parameters<typeof captureCaseScreenshot>[2]>;

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((complete) => { resolve = complete; });
  return { promise, resolve };
}

function caseInfo(id: string) {
  return {
    outputPath: vi.fn<TestInfo["outputPath"]>((...segments) => `/case-output/${id}/${segments.join("/")}`),
    attach: vi.fn<TestInfo["attach"]>(async () => {}),
  } satisfies Pick<TestInfo, "outputPath" | "attach">;
}

describe("case screenshot evidence", () => {
  it("retains the original basename, capture options and exact screenshot bytes", async () => {
    const bytes = Buffer.from("original PNG bytes"), info = caseInfo("desktop-claim");
    const screenshot = vi.fn<Page["screenshot"]>(async () => bytes);
    const options: CaptureOptions = {
      fullPage: true, animations: "disabled", caret: "hide", scale: "css",
      omitBackground: true, timeout: 10_000, type: "png",
    };

    const result = await captureCaseScreenshot({ screenshot }, ".scratch/dev/nested/claim-final.png", options, info);

    expect(result).toBe(bytes);
    expect(info.outputPath).toHaveBeenCalledExactlyOnceWith("claim-final.png");
    expect(screenshot).toHaveBeenCalledExactlyOnceWith({ ...options, path: "/case-output/desktop-claim/claim-final.png" });
    expect(info.attach).toHaveBeenCalledExactlyOnceWith("claim-final.png", {
      path: "/case-output/desktop-claim/claim-final.png", contentType: "image/png",
    });
  });

  it("never attaches evidence while the screenshot capture is still pending", async () => {
    const captured = deferred<Buffer>(), info = caseInfo("pending-capture");
    const screenshot = vi.fn<Page["screenshot"]>(() => captured.promise);

    const result = captureCaseScreenshot({ screenshot }, "review.png", {}, info);

    expect(screenshot).toHaveBeenCalledOnce();
    expect(info.attach).not.toHaveBeenCalled();
    captured.resolve(Buffer.from("captured review"));
    await result;
    expect(info.attach).toHaveBeenCalledOnce();
  });

  it("does not attach a stale output image when the current capture fails", async () => {
    const info = caseInfo("failed-capture"), failure = new Error("current capture failed");
    const retained = new Map([["/case-output/failed-capture/review.png", Buffer.from("stale earlier image")]]);
    const screenshot = vi.fn<Page["screenshot"]>(async () => { throw failure; });

    await expect(captureCaseScreenshot({ screenshot }, ".scratch/dev/review.png", {}, info)).rejects.toBe(failure);

    expect(retained.get("/case-output/failed-capture/review.png")).toEqual(Buffer.from("stale earlier image"));
    expect(info.attach).not.toHaveBeenCalled();
  });

  it("finishes only after the reporter has retained the screenshot attachment", async () => {
    const retained = deferred<void>(), attachmentStarted = deferred<void>(), info = caseInfo("pending-attachment");
    const bytes = Buffer.from("retained PNG"), screenshot = vi.fn<Page["screenshot"]>(async () => bytes);
    info.attach.mockImplementation(async () => { attachmentStarted.resolve(); await retained.promise; });
    let finished = false;
    const result = captureCaseScreenshot({ screenshot }, "publication.png", {}, info).then((value) => {
      finished = true;
      return value;
    });

    await attachmentStarted.promise;
    expect(finished).toBe(false);
    retained.resolve();
    expect(await result).toBe(bytes);
    expect(finished).toBe(true);
  });

  it("propagates attachment retention failures instead of reporting captured evidence as retained", async () => {
    const info = caseInfo("failed-attachment"), failure = new Error("reporter could not retain evidence");
    const screenshot = vi.fn<Page["screenshot"]>(async () => Buffer.from("captured PNG"));
    info.attach.mockRejectedValue(failure);

    await expect(captureCaseScreenshot({ screenshot }, "receipt.png", {}, info)).rejects.toBe(failure);

    expect(screenshot).toHaveBeenCalledOnce();
    expect(info.attach).toHaveBeenCalledOnce();
  });

  it("isolates simultaneous cases using the same basename without writing the shared legacy path", async () => {
    const first = caseInfo("desktop-first"), second = caseInfo("mobile-second"), images = new Map<string, Buffer>();
    const capture = (bytes: Buffer) => vi.fn<Page["screenshot"]>(async (options?: ScreenshotOptions) => {
      if (!options?.path) throw new Error("owned output path is required");
      images.set(options.path, bytes);
      return bytes;
    });
    const desktop = Buffer.from("desktop evidence"), mobile = Buffer.from("mobile evidence");

    await Promise.all([
      captureCaseScreenshot({ screenshot: capture(desktop) }, ".scratch/dev/final.png", {}, first),
      captureCaseScreenshot({ screenshot: capture(mobile) }, ".scratch/dev/final.png", {}, second),
    ]);

    expect([...images.keys()].sort()).toEqual(["/case-output/desktop-first/final.png", "/case-output/mobile-second/final.png"]);
    expect(images.get("/case-output/desktop-first/final.png")).toBe(desktop);
    expect(images.get("/case-output/mobile-second/final.png")).toBe(mobile);
    expect(images.has(".scratch/dev/final.png")).toBe(false);
    expect(first.attach).toHaveBeenCalledExactlyOnceWith("final.png", { path: "/case-output/desktop-first/final.png", contentType: "image/png" });
    expect(second.attach).toHaveBeenCalledExactlyOnceWith("final.png", { path: "/case-output/mobile-second/final.png", contentType: "image/png" });
  });

  it("leaves caller-owned screenshot options and nested clipping settings unchanged", async () => {
    const clip = Object.freeze({ x: 12, y: 18, width: 320, height: 240 });
    const options = Object.freeze({ clip, animations: "disabled" as const, timeout: 12_000 });
    const info = caseInfo("immutable-options"), screenshot = vi.fn<Page["screenshot"]>(async () => Buffer.from("clipped PNG"));

    await captureCaseScreenshot({ screenshot }, "clip.png", options, info);

    expect(options).toEqual({ clip: { x: 12, y: 18, width: 320, height: 240 }, animations: "disabled", timeout: 12_000 });
    expect(options).not.toHaveProperty("path");
    expect(screenshot.mock.calls[0][0]?.clip).toBe(clip);
  });
});

describe("workflow UI observation serialization", () => {
  const observation = () => serializeWorkflowUiObservation({
    routeSource: "packages/feature-finance/src/routes/holdings/[holdingSlug]/finance/documents/sales/invoices/page.tsx",
    targetSource: "packages/feature-finance/src/components/sales-controls.tsx",
    checks: ["loaded", "surface:packages/feature-finance/src/components/sales-controls.tsx:abc"],
    ownerContext: { companyId: "fictional-company", assignment: "owner" },
    authorityBranch: "allowed:actual-native-Company-holding_owner",
    authentication: "normal-native-owner",
    run: "012345abcdef",
    candidate: "a".repeat(40),
    sourceFingerprint: "b".repeat(64),
    verificationFingerprint: "c".repeat(64),
    releaseId: "2026-10-19T00-00-00-000Z",
    releaseManifestSha256: "d".repeat(64),
    targetKind: "route",
    targetFingerprint: "e".repeat(64),
    url: "http://127.0.0.1:3104/holdings/simulation/finance/documents/sales/invoices",
    viewport: { width: 390, height: 900 },
    theme: "light",
    resolvedTheme: "light",
    screenshot: { path: ".scratch/dev/test-suites/browser/run/artifacts/screen.png", sha256: "f".repeat(64), bytes: 1024 },
  });

  it("serializes explicit route, owner, release and screenshot bindings", () => {
    expect(observation()).toMatchObject({
      schema: "erpx-ui-browser-observation/v1",
      result: "passed",
      targetKind: "route",
      targetSource: "packages/feature-finance/src/routes/holdings/[holdingSlug]/finance/documents/sales/invoices/page.tsx",
      routeSource: "packages/feature-finance/src/routes/holdings/[holdingSlug]/finance/documents/sales/invoices/page.tsx",
      ownerContext: JSON.stringify({ companyId: "fictional-company", assignment: "owner" }),
      authorityBranch: "allowed:actual-native-Company-holding_owner",
      authentication: "normal-native-owner",
      screenshot: { path: ".scratch/dev/test-suites/browser/run/artifacts/screen.png", bytes: 1024 },
    });
  });

  it("rejects duplicate checks and evidence outside the compiled cohort", () => {
    expect(() => serializeWorkflowUiObservation({ ...observation(), checks: ["loaded", "loaded"] })).toThrow("UI_WORKFLOW_CHECKS_REQUIRED");
    expect(() => serializeWorkflowUiObservation({ ...observation(), url: "https://console-dev-erpx.jesadakorn.com/" })).toThrow("UI_WORKFLOW_COMPILED_ORIGIN_REQUIRED");
  });
});
