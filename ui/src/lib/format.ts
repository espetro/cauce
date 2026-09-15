export function domainOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "") || url;
  } catch {
    return url;
  }
}

export function faviconFor(url: string): string {
  const d = domainOf(url);
  return `https://icons.duckduckgo.com/ip3/${encodeURIComponent(d)}.ico`;
}

export function fmtDur(s: number | null | undefined): string {
  if (s == null || s < 0) return "0s";
  if (s < 60) return `${Math.floor(s)}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86400)}d`;
}

export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

export function fmtTs(epoch: number | null | undefined): string {
  if (!epoch) return "—";
  return new Date(epoch * 1000).toISOString().replace("T", " ").slice(0, 19) + " UTC";
}

export function truncate(s: string, n: number): string {
  return s.length > n ? `${s.slice(0, n - 1)}…` : s;
}
