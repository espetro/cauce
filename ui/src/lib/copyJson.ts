/**
 * Rung-3 `useReducer` FSM for the History screen's per-row "copy json" affordance
 * (history.md: "re-fetches the query from the search endpoint ... and copies the ... payload
 * to the clipboard"). Four states -- more than the state ladder's "trivial" bar for a plain
 * `useState`, so this is a discriminated union with an exhaustive `never` default, pure and
 * unit-testable with no renderer, per `ui/AGENTS.md`'s state ladder.
 */

export type CopyJsonState =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'copied' }
  | { status: 'error'; message: string }

export type CopyJsonAction =
  | { type: 'start' }
  | { type: 'success' }
  | { type: 'failure'; message: string }
  | { type: 'reset' }

export const initialCopyJsonState: CopyJsonState = { status: 'idle' }

export function copyJsonReducer(_state: CopyJsonState, action: CopyJsonAction): CopyJsonState {
  switch (action.type) {
    case 'start':
      return { status: 'loading' }
    case 'success':
      return { status: 'copied' }
    case 'failure':
      return { status: 'error', message: action.message }
    case 'reset':
      return { status: 'idle' }
    default: {
      const exhaustive: never = action
      return exhaustive
    }
  }
}
