# OpenCode Go provider research

Research date: 2026-09-27. Sources are first-party OpenCode and Meta documentation. No credential values were used or recorded.

## Provider mapping

- Provider/display name: **OpenCode Go**.
- OpenCode provider ID: `opencode-go`.
- API family: OpenAI **Responses API** (`@ai-sdk/openai`).
- Base URL for clients that append the protocol path: `https://opencode.ai/zen/go/v1`.
- Full inference endpoint: `https://opencode.ai/zen/go/v1/responses`.
- Direct API model ID: `muse-spark-1.3-contributor`.
- OpenCode configuration model reference: `opencode-go/muse-spark-1.3-contributor`.
- Live model catalog: `https://opencode.ai/zen/go/v1/models`.
- Authentication: send the API key unchanged as a bearer credential. Both legacy `sk-...` workspace keys and new `oc_sk_...` Console keys exist. The current gateway explicitly recognizes the new prefix and proxies `/zen/go/v1/responses` to the Go Responses backend, so a new Console key does not require a different endpoint.

The endpoint, model ID, SDK family, namespaced OpenCode model-reference format, and live model-catalog URL are documented in the [OpenCode Go guide](https://opencode.ai/docs/go). Bearer parsing, both key generations, and the compatibility proxy are visible in OpenCode's [current gateway source](https://github.com/anomalyco/opencode/blob/v1.18.32/packages/console/app/src/lib/inference-proxy.ts); the `oc_sk_...` masked key form also appears in its [Usage API documentation](https://opencode.ai/v2/docs/console/usage/).

## Relevant restrictions

- OpenCode Go is currently described as a **$10/month** plan; only one member per workspace may subscribe.
- The service is intended for coding-agent traffic. A client should identify itself with its own user agent and preserve a stable conversation/session identifier; OpenCode says traffic is monitored for abuse.
- Muse Spark 1.3 Contributor is region-limited under Meta's Geographic Use Policy.
- Contributor traffic is **not zero-data-retention**. OpenCode states that prompts and completions may be used to train future Meta models; Meta describes this as the tradeoff for the discounted Contributor tier.
- OpenCode currently lists a `$60` model-specific monthly usage allowance, with rolling caps equal to 20% per five hours and 50% per week. The documented token accounting prices are `$0.10` input, `$0.20` output, and `$0.002` cached read per one million tokens. OpenCode warns that model availability and usage limits may change.
- Reaching a Go limit blocks paid-model requests unless the workspace enables fallback to a funded Zen balance; free models remain available.

Sources: [OpenCode Go](https://opencode.ai/docs/go), [OpenCode providers](https://opencode.ai/docs/providers), [OpenCode Console inference](https://opencode.ai/v2/docs/console/inference/), [OpenCode Console usage API](https://opencode.ai/v2/docs/console/usage/), and [Meta pricing and rate limits](https://dev.meta.ai/docs/pricing-rate-limits).
