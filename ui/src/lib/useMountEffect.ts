import { useEffect } from "preact/hooks";

/** Escape hatch for one-time external sync on mount (setup + cleanup).
 * Wraps useEffect with an empty dependency array to make intent explicit. */
export function useMountEffect(effect: () => void | (() => void)) {
  // eslint-disable-next-line no-restricted-syntax
  useEffect(effect, []);
}
