import { useEffect, useState } from "preact/hooks";
import { useLocation } from "preact-iso";
import { Center, Header, ModeToggle, useAiAvailable, usePageTitle } from "../components/Header";
import { SearchBox } from "../features/suggests/SearchBox";

const MODE_KEY = "oxe-mode";
type Mode = "traditional" | "ai";

export default function Home() {
  usePageTitle("");
  const { path, route } = useLocation();
  const [q, setQ] = useState("");
  const [mode, setMode] = useState<Mode>(() =>
    localStorage.getItem(MODE_KEY) === "ai" ? "ai" : "traditional",
  );
  const aiAvailable = useAiAvailable();

  useEffect(() => {
    if (mode === "ai" && aiAvailable === false) setMode("traditional");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [aiAvailable]);

  useEffect(() => {
    localStorage.setItem(MODE_KEY, mode);
  }, [mode]);

  const submit = (query: string) => {
    const trimmed = query.trim();
    if (!trimmed) return;
    route(
      mode === "ai"
        ? `/search?q=${encodeURIComponent(trimmed)}&mode=ai`
        : `/search?q=${encodeURIComponent(trimmed)}`,
    );
  };

  return (
    <div class="min-h-screen flex flex-col">
      <Header path={path} />
      <main class="flex-1 flex items-center justify-center">
        <Center vh>
          <h1 class="text-4xl font-semibold tracking-tight mb-2">oxe</h1>
          <p class="opacity-50 text-sm mb-8">search the web, locally cached</p>
          <SearchBox value={q} onInput={setQ} onSubmit={submit} autoFocus size="lg" />
          <div class="mt-6 flex flex-col items-center gap-2">
            <ModeToggle mode={mode} onChange={setMode} aiAvailable={aiAvailable} />
            <p class="text-[13px] opacity-50 text-center px-4">
              {mode === "ai"
                ? "AI: streaming answer with cited sources"
                : "traditional: classic link results, cache metadata"}
            </p>
          </div>
        </Center>
      </main>
    </div>
  );
}
