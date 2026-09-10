type D1Summary = { uuid: string; name: string };

const d1Names = [
  "erpx-control-write",
  "erpx-control-read",
  "erpx-h001-write",
  "erpx-h001-read",
  "erpx-h002-write",
  "erpx-h002-read",
] as const;
const r2Names = [
  "erpx-h001-files-dev",
  "erpx-h002-files-dev",
  "erpx-backups-dev",
] as const;
const queueNames = ["erpx-events-dev", "erpx-events-dlq-dev"] as const;

function wrangler(args: string[], capture = false) {
  const result = Bun.spawnSync(["bunx", "wrangler", ...args], {
    stdout: capture ? "pipe" : "inherit",
    stderr: capture ? "pipe" : "inherit",
    env: { ...process.env, WRANGLER_SEND_METRICS: "false" },
  });
  const stdout = capture ? new TextDecoder().decode(result.stdout) : "";
  const stderr = capture ? new TextDecoder().decode(result.stderr) : "";
  if (result.exitCode !== 0)
    throw new Error(stderr || stdout || `wrangler ${args.join(" ")} failed`);
  return stdout;
}

function parseJson<T>(output: string): T {
  const start = Math.min(
    ...[output.indexOf("["), output.indexOf("{")].filter((value) => value >= 0),
  );
  if (!Number.isFinite(start))
    throw new Error(`Wrangler did not return JSON: ${output}`);
  return JSON.parse(output.slice(start)) as T;
}

function alreadyExists(error: unknown) {
  return (
    error instanceof Error &&
    /already exists|already in use/i.test(error.message)
  );
}

wrangler(["whoami"], true);

let databases = parseJson<D1Summary[]>(
  wrangler(["d1", "list", "--json"], true),
);
for (const name of d1Names) {
  if (!databases.some((database) => database.name === name)) {
    console.log(`Creating D1 database ${name}`);
    wrangler(["d1", "create", name]);
  }
}
databases = parseJson<D1Summary[]>(wrangler(["d1", "list", "--json"], true));

for (const name of r2Names) {
  try {
    wrangler(["r2", "bucket", "create", name], true);
    console.log(`Created R2 bucket ${name}`);
  } catch (error) {
    if (!alreadyExists(error)) throw error;
    console.log(`Reusing R2 bucket ${name}`);
  }
}

for (const name of queueNames) {
  try {
    wrangler(["queues", "create", name], true);
    console.log(`Created queue ${name}`);
  } catch (error) {
    if (!alreadyExists(error)) throw error;
    console.log(`Reusing queue ${name}`);
  }
}

const idFor = (name: string) => {
  const database = databases.find((entry) => entry.name === name);
  if (!database)
    throw new Error(`D1 database ${name} was not found after provisioning.`);
  return database.uuid;
};
const d1Bindings = [
  ["CONTROL_DB", "erpx-control-write", "migrations/control"],
  ["CONTROL_READ_DB", "erpx-control-read", "migrations/control-read"],
  ["H001_DB", "erpx-h001-write", "migrations/holding-write"],
  ["H001_READ_DB", "erpx-h001-read", "migrations/holding-read"],
  ["H002_DB", "erpx-h002-write", "migrations/holding-write"],
  ["H002_READ_DB", "erpx-h002-read", "migrations/holding-read"],
].map(([binding, database_name, migrations_dir]) => ({
  binding,
  database_name,
  database_id: idFor(database_name),
  migrations_dir,
}));

const appConfig = JSON.parse(await Bun.file("wrangler.jsonc").text());
appConfig.env.dev = {
  name: "erpx-dev",
  vars: {
    ERPX_ENVIRONMENT: "dev",
    BETTER_AUTH_URL: process.env.ERPX_DEV_URL ?? "https://erpx-dev.workers.dev",
  },
  assets: appConfig.assets,
  d1_databases: d1Bindings,
  r2_buckets: [
    { binding: "H001_FILES", bucket_name: r2Names[0] },
    { binding: "H002_FILES", bucket_name: r2Names[1] },
    { binding: "BACKUP_FILES", bucket_name: r2Names[2] },
  ],
  queues: { producers: [{ binding: "ERPX_EVENTS", queue: queueNames[0] }] },
};
await Bun.write("wrangler.jsonc", `${JSON.stringify(appConfig, null, 2)}\n`);

const jobsConfig = JSON.parse(await Bun.file("wrangler.jobs.jsonc").text());
jobsConfig.env.dev = {
  name: "erpx-jobs-dev",
  triggers: jobsConfig.triggers,
  d1_databases: d1Bindings.filter(({ binding }) => binding.startsWith("H")),
  r2_buckets: appConfig.env.dev.r2_buckets.filter(
    ({ binding }: { binding: string }) => binding.startsWith("H"),
  ),
  queues: {
    producers: [{ binding: "ERPX_EVENTS", queue: queueNames[0] }],
    consumers: [
      {
        queue: queueNames[0],
        max_batch_size: 20,
        max_retries: 5,
        dead_letter_queue: queueNames[1],
      },
    ],
  },
};
await Bun.write(
  "wrangler.jobs.jsonc",
  `${JSON.stringify(jobsConfig, null, 2)}\n`,
);

console.log(
  "ERPX development resources are provisioned and their bindings are recorded.",
);

export {};
