import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { resolve, join } from "node:path";

// Isolated, loopback-only lab. Original VM disks and the production service stay intact.
const directory = resolve(".scratch/ucs-lab");
mkdirSync(directory, { recursive: true, mode: 0o700 });
const disk = join(directory, "ucs-lab.qcow2"),
  pidFile = join(directory, "qemu.pid");
function run(args: string[]) {
  const result = Bun.spawnSync(args, { stdout: "pipe", stderr: "pipe" });
  if (result.exitCode !== 0) throw new Error("LAB_COMMAND_FAILED: " + args[0]);
  return result.stdout.toString().trim();
}
if (existsSync(pidFile)) {
  const pid = readFileSync(pidFile, "utf8").trim();
  if (/^\d+$/.test(pid)) {
    const result = Bun.spawnSync(["ps", "-p", pid, "-o", "command="], {
      stdout: "pipe",
      stderr: "pipe",
    });
    if (result.exitCode === 0 && result.stdout.toString().includes(disk)) {
      console.log(
        "UCS lab is already running. HTTPS 18443, SSH 18322, UCS LDAPS 18636, Samba LDAPS 18637 (loopback only).",
      );
      process.exit(0);
    }
  }
}
if (process.argv.includes("--status")) {
  console.log("UCS lab is stopped.");
  process.exit(0);
}
for (const port of [18322, 18443, 18389, 18636, 18637]) {
  const result = Bun.spawnSync(
    ["lsof", "-nP", "-iTCP:" + port, "-sTCP:LISTEN"],
    { stdout: "pipe", stderr: "pipe" },
  );
  if (result.exitCode === 0) throw new Error("LAB_PORT_OCCUPIED: " + port);
}
if (!existsSync(disk)) {
  const source = process.argv[2];
  if (!source || source.startsWith("--"))
    throw new Error("Pass the existing UCS image directory on first start.");
  const root = resolve(source),
    overlay = join(root, "ucs-overlay-rescue.qcow2"),
    base = join(root, "UCS-KVM-Image.qcow2");
  if (!existsSync(overlay) || !existsSync(base))
    throw new Error("UCS_IMAGES_REQUIRED");
  if (process.platform !== "darwin")
    throw new Error("This local lab launcher requires macOS APFS cloning.");
  run(["cp", "-c", overlay, disk]);
  run([
    "qemu-img",
    "rebase",
    "-u",
    "-f",
    "qcow2",
    "-F",
    "qcow2",
    "-b",
    base,
    disk,
  ]);
}
run([
  "qemu-system-x86_64",
  "-name",
  "erpx-ucs-lab",
  "-machine",
  "q35,accel=tcg",
  "-cpu",
  "max",
  "-smp",
  "2",
  "-m",
  "4096",
  "-drive",
  "file=" + disk + ",format=qcow2,if=virtio",
  "-device",
  "virtio-net-pci,netdev=net0",
  "-netdev",
  "user,id=net0,hostfwd=tcp:127.0.0.1:18322-:22,hostfwd=tcp:127.0.0.1:18443-:443,hostfwd=tcp:127.0.0.1:18389-:7389,hostfwd=tcp:127.0.0.1:18636-:7636,hostfwd=tcp:127.0.0.1:18637-:636",
  "-display",
  "none",
  "-serial",
  "file:" + join(directory, "serial.log"),
  "-qmp",
  "unix:" + join(directory, "qmp.sock") + ",server=on,wait=off",
  "-pidfile",
  pidFile,
  "-daemonize",
]);
console.log(
  "UCS lab started in the background. Wait for boot, then test the connector in People settings.",
);
