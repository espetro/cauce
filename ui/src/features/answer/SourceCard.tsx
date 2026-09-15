import type { AiSource } from "../../lib/ai";
import { recordClick } from "../../lib/api";

/** Compact horizontal tile: number badge, favicon, domain, truncated title. */
export function SourceCard({
  source,
  n,
  queryHash,
}: {
  source: AiSource;
  n: number;
  queryHash: string;
}) {
  let domain = source.url;
  try {
    domain = new URL(source.url).hostname;
  } catch {
    // keep raw url
  }
  return (
    <a
      id={`src-${n}`}
      href={source.url}
      target="_blank"
      rel="noopener noreferrer"
      onClick={() =>
        recordClick({
          query_hash: queryHash,
          result_id: `src-${n}`,
          url: source.url,
          title: source.title,
        })
      }
      class="card card-compact bg-base-200 border border-base-300 w-[150px] shrink-0 snap-start hover:opacity-90 transition-opacity"
    >
      <div class="card-body p-2.5 gap-1">
        <div class="flex items-center gap-1.5 text-[11px] opacity-70">
          <span class="badge badge-xs badge-primary font-mono">{n}</span>
          <img
            src={`https://icons.duckduckgo.com/ip3/${domain}.ico`}
            alt=""
            width={12}
            height={12}
            loading="lazy"
            onError={(e) => (e.currentTarget.style.display = "none")}
          />
          <span class="truncate">{domain}</span>
        </div>
        <p class="text-[12px] leading-snug line-clamp-2">{source.title}</p>
      </div>
    </a>
  );
}
