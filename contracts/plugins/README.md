# Contribute an ERPX plugin

Create a public GitHub repository with the `erpx-plugin` topic and an `erpx-plugin.json` file at its root. Use the [site measurement example](site-measurement.example.json) for the supported manifest fields. Choose a three-part version such as `1.0.0` and commit the manifest.

Open a pull request against the ERPX `codex/rust-backend-migration` branch. Add an entry to [the approval registry](registry.json) with the manifest's `id` and `version`, the GitHub `repositoryId` and lowercase `repository` name, the full `commit`, and the `sha256` of the committed manifest bytes. An ERPX project owner must review this exact version before it can be merged and installed.

Company administrators find approved versions in Apps or paste the GitHub repository link there. A Company installation is scoped to that Company. Uninstalling it removes its extension while the CRM activity remains governed by its existing access rules.

The example reads an open CRM meeting with the subject beginning “Site measurement” or “นัดวัด”. The appointment must have a committed CRM outbox event. ERPX applies Company, workspace, package and CRM role checks before a person can see the link to the existing activity.
