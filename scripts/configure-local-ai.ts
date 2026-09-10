import { chmod } from "node:fs/promises";
const path = ".env.local";
const current = (await Bun.file(path).exists())
  ? await Bun.file(path).text()
  : "";
if (/^AI_CREDENTIAL_KEYS=.+/m.test(current)) {
  console.log("Local AI credential encryption is already configured.");
} else {
  const encoded = Buffer.from(
    crypto.getRandomValues(new Uint8Array(32)),
  ).toString("base64");
  await Bun.write(
    path,
    `${current.trimEnd()}\nAI_CREDENTIAL_KEY_ID=v1\nAI_CREDENTIAL_KEYS='${JSON.stringify({ v1: encoded })}'\n`,
  );
  await chmod(path, 0o600);
  console.log(
    "Local AI credential encryption configured in the ignored environment file.",
  );
}
// OpenNext intentionally excludes .env files from its Worker bindings. Keep
// local encryption identical in Next development, preview and the jobs Worker.
const workerPath = ".dev.vars";
const workerCurrent = (await Bun.file(workerPath).exists())
  ? await Bun.file(workerPath).text()
  : "";
const credentialLines = (await Bun.file(path).text())
  .split("\n")
  .filter((line) => /^AI_CREDENTIAL_(KEY_ID|KEYS)=/.test(line));
await Bun.write(
  workerPath,
  workerCurrent
    .split("\n")
    .filter((line) => !/^AI_CREDENTIAL_(KEY_ID|KEYS)=/.test(line))
    .join("\n")
    .trimEnd() +
    "\n" +
    credentialLines.join("\n") +
    "\n",
);
await chmod(workerPath, 0o600);
console.log(
  "Local application and AI worker share the same credential keyring.",
);
export {};
