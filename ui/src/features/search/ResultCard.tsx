import type { SearchResult } from "../../lib/schemas";
import { domainOf, faviconFor, truncate } from "../../lib/format";

interface Props {
  result: SearchResult;
  queryHash: string;
  onOpen: (r: SearchResult) => void;
}

/** Card-less Google-anatomy result: favicon + domain, blue title,
 * two-line snippet, collapsed cached text preview. */
export function ResultCard({ result, onOpen }: Props) {
  const url = result.url ?? "";
  const domain = domainOf(url);
  const snippet = (result.text || result.highlights?.join(" ") || "").trim();
  const title = result.title || "(untitled)";
  return (
    <article class="py-3">
      <div class="flex items-center gap-2 text-[13px] opacity-70">
        <img
          src={faviconFor(url)}
          alt=""
          width={16}
          height={16}
          loading="lazy"
          class="inline-block"
          onError={(e) => ((e.target as HTMLImageElement).style.display = "none")}
        />
        <span class="truncate">{domain}</span>
      </div>
      <h3 class="text-lg leading-snug my-0.5">
        <a
          href={url}
          target="_blank"
          rel="noopener noreferrer"
          class="link link-primary no-underline font-medium"
          onClick={() => onOpen(result)}
          data-result-id={result.id || url}
        >
          {truncate(title, 120)}
        </a>
      </h3>
      {snippet && <p class="text-sm opacity-80 line-clamp-2 m-0">{truncate(snippet, 200)}</p>}
      {snippet && (
        <details class="text-sm">
          <summary class="opacity-50 cursor-pointer select-none text-[13px]">
            cached page text preview
          </summary>
          <p class="opacity-70 m-1 whitespace-pre-wrap">{truncate(snippet, 400)}</p>
        </details>
      )}
    </article>
  );
}
