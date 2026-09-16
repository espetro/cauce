/** @jsxImportSource preact */
import { Component, type ComponentChildren } from "preact";

/** Route-level class boundary (mounted in routes/_layout.tsx around the
 * routed content): on a render crash it replaces the page area with a
 * 500-ish hero while keeping the app shell (header, toasts) alive. */
export class ErrorBoundary extends Component<
  { children: ComponentChildren },
  { error: Error | null }
> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error) {
    console.error("ui error boundary:", error);
  }

  render() {
    if (this.state.error) {
      return (
        <div class="flex-1 flex flex-col items-center justify-center min-h-[60vh] px-4 text-center">
          <div class="max-w-md">
            <p class="text-5xl font-semibold font-mono tracking-tight opacity-30">500</p>
            <h1 class="mt-2 text-lg font-medium">something broke</h1>
            <p class="py-3 text-sm opacity-60">{this.state.error.message}</p>
            <div class="mt-3 flex items-center justify-center gap-2">
              <button
                type="button"
                class="btn btn-primary btn-sm"
                onClick={() => this.setState({ error: null })}
              >
                try again
              </button>
              <a href="/" class="btn btn-ghost btn-sm">
                back to search
              </a>
            </div>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
