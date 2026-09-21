# later: SearXNG as an upstream engine
Issue: #67

Seam: declarative runtime `parse.kind: json` (W1-02).
Trigger: a user already runs SearXNG and wants oxe's cache/MCP/observability in front of its
70 engines; or oxe needs a tier-2 breadth source without writing 20 specs.
Shape: `engines/searxng.yaml` hitting `<instance>/search?q={q}&format=json`, mapping
`results[]`, tier 2, disabled by default, instance URL from config. Result `engine` field
becomes `searxng:<upstream engine>`.
