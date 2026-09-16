import { useEffect, useState } from "preact/hooks";
import { useLocation } from "preact-iso";
import { Center, usePageTitle } from "../components/Header";
import { useModels, type Mode } from "../components/ModeSegments";
import { SearchBox } from "../features/suggests/SearchBox";

const MODE_KEY = "oxe-mode";

export default function Home() {
  usePageTitle("");
  const { route } = useLocation();
  const [q, setQ] = useState("");
  const [mode, setMode] = useState<Mode>(() =>
    typeof localStorage === "undefined"
      ? "traditional"
      : localStorage.getItem(MODE_KEY) === "ai"
        ? "ai"
        : "traditional",
  );
  const { available: aiAvailable, models, error: modelsError } = useModels();

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
    <Center vh>
      <h1 class="text-5xl font-semibold tracking-tight mb-4">oxe</h1>
      <p class="opacity-50 text-sm mb-6 max-md:mb-4 md:mb-10">your local web intel layer</p>
      <div class="self-stretch flex justify-center px-3 min-w-0 mb-8">
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
          modelsError={modelsError}
        />
      </div>
    </Center>
  );
}
