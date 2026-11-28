# Run the Service Count demo

Use a fictional Company to create a draft count for Service parts and reopen it after saving.

## Before you start

The qualified contributor environment uses macOS ARM64 with the dedicated Colima profile `erpx-opensource-dev`, Git, and standalone `docker-compose`. Follow the [environment setup](contracts/next/service-count-devcontainer.md#supported-host-and-isolation) for the dedicated socket, disk headroom, and local port requirements. Install the Dev Containers CLI or editor integration if you want to attach your editor to the Linux source volume.

## Start the workspace

From the root of your checkout:

```sh
export ERPX_DEV_WORKSPACE_KEY=my-count-arm64-rust1991
export ERPX_DEV_SOURCE_REF=$(git rev-parse HEAD)
scripts/local/dev-container up --app=service
```

The first run installs locked dependencies and prepares fictional data in isolated PostgreSQL databases. Follow the [local HTTPS setup](contracts/next/service-count-devcontainer.md#https-and-normal-authentication), including trusting the workspace CA. The same section explains where to find the fictional Account credentials inside the workspace.

## Create a Count

1. Open [local Acme](https://acme.erpx.localhost:3443) and sign in as **Synthetic Count maker**.
2. Select Company `acme-co` and the maker role. Open **Field Service**, then its parts settings and initial stock count.
3. Choose the seeded warehouse and product, enter a quantity and unit cost, and save the draft. If the Company needs parts setup, follow the [Service Parts guide](contracts/next/service-parts-devcontainer.md).

You should see a Count reference and a **Draft** status. Open the saved Count again and confirm the quantity appears on the record.

## If you get stuck

- If the browser rejects the certificate, check that your certificate came from this workspace and that the URL contains the full hostname and port.
- If startup was interrupted, use the [documented recovery command](contracts/next/service-count-devcontainer.md#editor-and-cli-lifecycle).
- To stop the workspace and keep your data, run `scripts/local/dev-container down`.

[Find the code behind this screen](find-the-code.md)
