/** Full acceptance needs both boundary compliance and completed semantic owners. */
export async function runArchitectureAcceptance(execute: (command: string[]) => Promise<number>): Promise<void> {
  for (const command of [["scripts/architecture/check.ts"], ["scripts/architecture/status.ts", "--require-clean"]]) {
    if (await execute(command) !== 0) throw new Error(`ARCHITECTURE_ACCEPTANCE_FAILED:${command[0]}`);
  }
}

if (import.meta.main) {
  await runArchitectureAcceptance(async command => {
    const child = Bun.spawn([process.execPath, ...command], { cwd: process.cwd(), env: process.env, stdin: "inherit", stdout: "inherit", stderr: "inherit" });
    const onInt = () => child.kill("SIGINT"), onTerm = () => child.kill("SIGTERM");
    process.on("SIGINT", onInt); process.on("SIGTERM", onTerm);
    try { return await child.exited; }
    finally { process.off("SIGINT", onInt); process.off("SIGTERM", onTerm); }
  });
}
