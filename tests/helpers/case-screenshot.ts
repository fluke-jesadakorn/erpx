import { test, type Page, type TestInfo } from "@playwright/test";
import { basename } from "node:path";

export type CaseScreenshotOptions = Omit<
  NonNullable<Parameters<Page["screenshot"]>[0]>,
  "path" | "type"
> & { type?: "png" };

export async function captureCaseScreenshot(
  page: Pick<Page, "screenshot">,
  filename: string,
  options: CaseScreenshotOptions = {},
  info: Pick<TestInfo, "outputPath" | "attach"> = test.info(),
) {
  const screenshotName = basename(filename);
  const screenshotPath = info.outputPath(screenshotName);
  const buffer = await page.screenshot({ ...options, path: screenshotPath });
  await info.attach(screenshotName, { path: screenshotPath, contentType: "image/png" });
  return buffer;
}
