# PMS CPU profiling prerequisites — 2026-10-10

Investigation only; this note is not a performance pass or another delivery plan. The [active plan](../production-plan.md) retains read p95 <1s and ordinary command p95 <2s. Retained release evidence still fails guest.save at 2,272.183 ms.

Official [Node 24 CLI documentation](https://r2.nodejs.org/docs/latest-v24.x/api/cli.html) documents loopback Inspector access, `--inspect-port`, activation through SIGUSR1, and CPU profiling. [Inspector documentation](https://nodejs.org/api/inspector.html) documents `Profiler.enable/start/stop`, session disconnection and Inspector closure. Those pages currently describe Node 24.21.0 and 26.11.1 respectively; neither is a claim that the installed toolchain changed.

A local read-only API probe confirms pinned Node **24.20.0** exposes `inspector.Session`, `inspector.close`, `process.getBuiltinModule` and WebSocket. No listener or profiling session was started by this probe.

After the remaining functional browser diagnostics finish, capture a CPU profile from the exact owned disposable 3104 compiled cohort during the unchanged workload. Check PID, working directory and loopback endpoint before attaching; do not attach to development 3100, private production 3101, staging, or unrelated processes. Keep raw profiles private, close the endpoint afterward, and record instrumentation overhead. Use measured hot paths to choose an optimization. A profile run alone does not replace strict end-to-end latency or financial reconciliation evidence. Final aggregate native/security/browser/route checks must run on the resulting unchanged candidate after measured fixes; staged diagnostic passes do not establish final acceptance.
