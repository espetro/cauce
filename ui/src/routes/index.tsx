import { useEffect, useState } from "preact/hooks";
import { useLocation } from "preact-iso";
import { Center, Header, usePageTitle } from "../components/Header";
import { AboutHint } from "../components/AboutHint";
import { useModels, type Mode } from "../components/ModeSegments";
import { SearchBox } from "../features/suggests/SearchBox";

const MODE_KEY = "oxe-mode";

export default function Home() {
  usePageTitle("");
  const { path, route } = useLocation();
  const [q, setQ] = useState("");
  const [mode, setMode] = useState<Mode>(() =>
    localStorage.getItem(MODE_KEY) === "ai" ? "ai" : "traditional",
  );
  const { available: aiAvailable, models } = useModels();

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
          <p class="opacity-50 text-sm">your local web intel layer</p>
          <div class="flex items-start justify-center gap-1.5 mb-8 max-w-full px-3 min-w-0">
            <SearchBox
              value={q}
              onInput={setQ}
              onSubmit={submit}
              autoFocus
              size="lg"
              mode={mode}
              onModeChange={setMode}
              aiAvailable={aiAvailable}
              models={models}
            />
            <AboutHint />
          </div>
        </Center>
      </main>
    </div>
  );
}
