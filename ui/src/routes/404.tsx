import * as m from "../lib/i18n";

export default function NotFound() {
  return (
    <div class="flex-1 flex flex-col items-center justify-center min-h-[60vh] px-4 text-center animate-in fade-in zoom-in-95 duration-300">
      <p class="text-5xl font-logo font-semibold tracking-tight opacity-30">404</p>
      <h1 class="mt-2 text-lg font-medium">{m.notfound_title()}</h1>
      <p class="mt-1 text-sm opacity-60">{m.notfound_body()}</p>
      <div class="mt-6 flex items-center gap-2">
        <a href="/" class="btn btn-primary btn-sm">
          {m.notfound_back()}
        </a>
        <a href="/history" class="btn btn-ghost btn-sm">
          {m.notfound_history()}
        </a>
      </div>
    </div>
  );
}
