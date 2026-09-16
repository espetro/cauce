export default function NotFound() {
  return (
    <div class="flex-1 flex flex-col items-center justify-center min-h-[60vh] px-4 text-center animate-in fade-in zoom-in-95 duration-300">
      <p class="text-5xl font-logo font-semibold tracking-tight opacity-30">404</p>
      <h1 class="mt-2 text-lg font-medium">nothing here</h1>
      <p class="mt-1 text-sm opacity-60">
        this page does not exist — check the address or head back to search
      </p>
      <div class="mt-6 flex items-center gap-2">
        <a href="/" class="btn btn-primary btn-sm">
          back to search
        </a>
        <a href="/history" class="btn btn-ghost btn-sm">
          history
        </a>
      </div>
    </div>
  );
}
