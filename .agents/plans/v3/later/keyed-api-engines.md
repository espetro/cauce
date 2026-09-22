# later: keyed-API engine specs (brave-api, mojeek-api)
Issue: #95

Seam: the declarative engine runtime (W1-02); spec `request.headers` values may carry
`${env:NAME}`/`${file:PATH}` templates resolved through the config interpolator.
Trigger: residential-IP scraping of Brave/Bing is durably blocked (TLS fingerprint or IP
reputation) and a keyed API becomes the pragmatic fix.
Shape: ship disabled-by-default `engines/brave-api.yaml` (and `mojeek-api.yaml` if
adopted) as ordinary declarative specs with `parse.kind: json` and the key in a templated
header; no runtime changes beyond W1-02. Must not weaken the politeness/health contracts.
