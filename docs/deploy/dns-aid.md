# DNS for AI Discovery (DNS-AID) & Agent Discovery Records

DNS-AID allows AI agents to discover agent endpoints, tools, and capability catalogs directly via DNS-over-HTTPS queries before establishing an HTTP connection.

## Required DNS Records for `cauce.fyi`

Add the following records in your DNS management zone (e.g., Cloudflare DNS):

### 1. Agent Entrypoint (HTTPS / SVCB)

- **Type**: `HTTPS` (or `SVCB`)
- **Name**: `_index._agents` (fully qualified: `_index._agents.cauce.fyi`)
- **Priority**: `1`
- **Target**: `cauce.fyi`
- **Value**: `alpn="h2,h3" port=443`

In BIND zone format:
```dns
_index._agents.cauce.fyi. 3600 IN HTTPS 1 cauce.fyi. alpn="h2,h3" port=443
```

### 2. A2A (Agent-to-Agent) Protocol Endpoint

- **Type**: `HTTPS` (or `SVCB`)
- **Name**: `_a2a._agents` (fully qualified: `_a2a._agents.cauce.fyi`)
- **Priority**: `1`
- **Target**: `cauce.fyi`
- **Value**: `alpn="a2a" port=443 mandatory=alpn,port`

In BIND zone format:
```dns
_a2a._agents.cauce.fyi. 3600 IN HTTPS 1 cauce.fyi. alpn="a2a" port=443 mandatory=alpn,port
```

### 3. Agent Resource Discovery (ARD) Manifest TXT Record

- **Type**: `TXT`
- **Name**: `_catalog._agents` (fully qualified: `_catalog._agents.cauce.fyi`)
- **Value**: `"url=https://cauce.fyi/.well-known/ai-catalog.json"`

In BIND zone format:
```dns
_catalog._agents.cauce.fyi. 3600 IN TXT "url=https://cauce.fyi/.well-known/ai-catalog.json"
```

## DNSSEC Requirement

DNS-AID resolvers validate records using DNSSEC.
Ensure DNSSEC is enabled for your zone:
1. In Cloudflare Dashboard: **DNS** → **Settings** → **DNSSEC** → **Enable DNSSEC**.
2. Add the DS record to your domain registrar if not using Cloudflare Registrar.
