// WebMCP declarative attribute definitions for Svelte elements.
import "svelte/elements";

declare module "svelte/elements" {
  export interface HTMLAttributes<T extends EventTarget> {
    toolname?: string;
    tooldescription?: string;
    toolparamdescription?: string;
  }
}
