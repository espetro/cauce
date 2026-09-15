import type { JSX } from "preact";

export interface Token {
  t: "text" | "code";
  v: string;
}

/** Tiny markdown-lite inline renderer: `code`, **bold**, *italic*, [n] citations. */
export function renderInline(text: string, keyBase: string): JSX.Element[] {
  const out: JSX.Element[] = [];
  const re = /(`[^`]+`|\*\*[^*]+\*\*|\*[^*]+\*|\[\d+\])/g;
  let last = 0;
  let m: RegExpExecArray | null;
  let i = 0;
  while ((m = re.exec(text)) !== null) {
    if (m.index > last)
      out.push(<span key={`${keyBase}-t${i++}`}>{text.slice(last, m.index)}</span>);
    const tok = m[0];
    if (tok.startsWith("`")) {
      out.push(
        <code key={`${keyBase}-c${i++}`} class="bg-base-200 px-1 rounded text-[0.9em]">
          {tok.slice(1, -1)}
        </code>,
      );
    } else if (tok.startsWith("**")) {
      out.push(<strong key={`${keyBase}-b${i++}`}>{tok.slice(2, -2)}</strong>);
    } else if (tok.startsWith("*")) {
      out.push(<em key={`${keyBase}-i${i++}`}>{tok.slice(1, -1)}</em>);
    } else {
      const n = Number(tok.slice(1, -1));
      out.push(
        <a
          key={`${keyBase}-r${i++}`}
          href={`#src-${n}`}
          class="citation text-primary text-[0.75em] align-super ml-0.5"
          onClick={(e) => {
            e.preventDefault();
            const el = document.getElementById(`src-${n}`);
            if (el) {
              el.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "center" });
              el.classList.add("outline", "outline-primary");
              setTimeout(() => el.classList.remove("outline", "outline-primary"), 1200);
            }
          }}
        >
          {n}
        </a>,
      );
    }
    last = m.index + tok.length;
  }
  if (last < text.length) out.push(<span key={`${keyBase}-t${i++}`}>{text.slice(last)}</span>);
  return out;
}

/** Minimal block-level markdown: paragraphs, bullet lists, headings. */
export function MarkdownLite({ text }: { text: string }) {
  const blocks: JSX.Element[] = [];
  const lines = text.split("\n");
  let para: string[] = [];
  let list: string[] = [];
  let k = 0;

  const flushPara = () => {
    if (para.length) {
      blocks.push(
        <p key={`p${k++}`} class="leading-relaxed whitespace-pre-wrap">
          {renderInline(para.join(" "), `p${k}`)}
        </p>,
      );
      para = [];
    }
  };
  const flushList = () => {
    if (list.length) {
      blocks.push(
        <ul key={`ul${k++}`} class="list-disc pl-5 space-y-1 my-2">
          {list.map((li, j) => (
            <li key={j}>{renderInline(li, `li${k}-${j}`)}</li>
          ))}
        </ul>,
      );
      list = [];
    }
  };

  for (const line of lines) {
    const t = line.trim();
    if (!t) {
      flushPara();
      flushList();
    } else if (/^[-*]\s+/.test(t)) {
      flushPara();
      list.push(t.replace(/^[-*]\s+/, ""));
    } else if (/^#{1,3}\s+/.test(t)) {
      flushPara();
      flushList();
      blocks.push(
        <h3 key={`h${k++}`} class="font-semibold mt-3 mb-1 text-base">
          {renderInline(t.replace(/^#{1,3}\s+/, ""), `h${k}`)}
        </h3>,
      );
    } else {
      if (list.length) flushList();
      para.push(t);
    }
  }
  flushPara();
  flushList();
  return <div class="text-[15px] space-y-2">{blocks}</div>;
}
