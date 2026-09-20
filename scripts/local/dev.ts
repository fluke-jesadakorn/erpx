// Public development assets may be cached by the proxy between runs. Give each
// dev process one stable deployment ID shared by its server and browser assets.
export {};
const child = Bun.spawn(["next", "dev", "-p", "3100"], {
  stdin: "inherit",
  stdout: "inherit",
  stderr: "inherit",
  env: {
    ...process.env,
    NEXT_DEPLOYMENT_ID: `dev-${Date.now()}-${crypto.randomUUID().slice(0, 8)}`,
  },
});
for (const signal of ["SIGINT", "SIGTERM"] as const)
  process.on(signal, () => child.kill(signal));
process.exitCode = await child.exited;
