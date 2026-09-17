This is the only directory a `useEffect` may live in. Every effect here is a reviewed,
named exception to the state ladder in `ui/AGENTS.md`: URL first, then non-opaque state,
then `useReducer` FSM, then `@xstate/store`. `.oxlintrc.json`'s `no-restricted-syntax`
rule bans `useEffect` everywhere else in `src/`.
