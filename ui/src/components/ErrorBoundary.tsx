/** @jsxImportSource preact */
import { Component, type ComponentChildren } from "preact";

/** Class boundary: renders a daisyUI hero fallback on render errors. */
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
        <div class="hero min-h-[60vh]">
          <div class="hero-content text-center">
            <div class="max-w-md">
              <h1 class="text-3xl font-semibold">something broke</h1>
              <p class="py-4 text-sm opacity-60">{this.state.error.message}</p>
              <button
                type="button"
                class="btn btn-primary btn-sm"
                onClick={() => location.reload()}
              >
                reload
              </button>
            </div>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
