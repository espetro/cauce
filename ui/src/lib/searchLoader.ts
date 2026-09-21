import { search } from './api.ts'
import { errorMessage } from './effects/continuousScroll.ts'
import type { components } from './types.gen.ts'

type SearxResponse = components['schemas']['SearxResponse']

interface SearchLoaderDeps {
  q: string
  mode: 'ai' | undefined
  force?: 'ai-off' | 'error' | 'empty' | null
}

export interface SearchLoadFailure {
  loadError: string
}

// AI mode skips classic search (it streams from POST /answer and `force` there is the /answer fixture),
// except when AI is unavailable: search.md keeps the classic results on screen under the notice.
export async function loadSearchPage(
  deps: SearchLoaderDeps,
  searchFn: typeof search = search,
): Promise<SearxResponse | SearchLoadFailure | null> {
  if (!deps.q || (deps.mode === 'ai' && deps.force !== 'ai-off')) {
    return null
  }
  const forceParams = deps.force ? `&force=${deps.force}` : ''
  // A failed search is a state, not an exception: rejecting would hit the router error boundary and log.
  try {
    return await searchFn({ q: deps.q }, new URLSearchParams(`q=${encodeURIComponent(deps.q)}${forceParams}`))
  } catch (error) {
    return { loadError: errorMessage(error) }
  }
}
