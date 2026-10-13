/** Compatibility entry point. Development fixtures only belong in local storage. */
if (process.argv.includes("--remote")) {
  throw new Error("Development credentials and fixtures cannot seed a remote tenant. Provision real tenant identities through an approved bootstrap/import workflow.");
}
const seed = Bun.spawn(["bun", "run", "seed:local"], { stdout: "inherit", stderr: "inherit" });
process.exit(await seed.exited);

export {};
