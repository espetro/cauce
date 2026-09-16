import { useState } from "preact/hooks";
import { Center, usePageTitle } from "../components/Header";
import { navigate } from "../lib/routes";
import { useModels, useSearchMode, type Mode } from "../components/ModeSegments";
import { SearchBox } from "../features/suggests/SearchBox";
import * as m from "../lib/i18n";

export default function Home() {
  usePageTitle("");
  const [q, setQ] = useState("");
  const [mode, setMode] = useSearchMode();
  const { available: aiAvailable, models, error: modelsError } = useModels();
  // AI unavailable: render Search results (do not demote stored preference).
  const effectiveMode: Mode = mode === "ai" && aiAvailable === false ? "traditional" : mode;

  const submit = (query: string) => {
    const trimmed = query.trim();
    if (!trimmed) return;
    // persist the mode preference at event time (useSearchMode stores it)
    setMode(mode);
    navigate("search", effectiveMode === "ai" ? { q: trimmed, mode: "ai" } : { q: trimmed });
  };

  return (
    <Center vh>
      <h1 class="text-5xl font-semibold tracking-tight mb-4">oxe</h1>
      <p class="opacity-50 text-sm mb-6 max-md:mb-4 md:mb-10">{m.home_tagline()}</p>
      <div class="self-stretch flex justify-center px-3 min-w-0 mb-8">
        <SearchBox
          value={q}
          onInput={setQ}
          onSubmit={submit}
          autoFocus
          size="lg"
          mode={effectiveMode}
          onModeChange={setMode}
          aiAvailable={aiAvailable}
          models={models}
          modelsError={modelsError}
        />
      </div>
    </Center>
  );
}
