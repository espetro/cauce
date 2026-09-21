# later: TypeScript engine SDK
Issue: #69

Seam: exec protocol (parent 4.3, W0-07); the Python SDK is the reference.
Trigger: an external contributor asks for it, or a Bun-based scraper is the fastest way to
prototype an engine.
Shape: `sdk/ts` with `defineScraper(fn)` over the same JSON lines protocol, `bun` runtime,
Apache-2.0. Conformance: the W0-07 echo-engine test runs against both SDKs.
