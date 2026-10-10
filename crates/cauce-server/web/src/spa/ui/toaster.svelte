<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ui/toaster` — the sonner toast outlet (svelte-sonner), one per app,
  mounted by the shell next to `UiConfirmHost`. Toasts are emitted
  through `lib/backend.svelte.ts`'s health transitions (and any later
  feature toasts via `import { toast } from "svelte-sonner"`); this
  wrapper pins the placement/theme and the app-token restyle lives in
  `app.css` next to the other global surface rules (the toaster portals
  outside the component tree, so scoped styles cannot reach it).
-->
<script lang="ts">
  import { Toaster } from "svelte-sonner";
  import { theme } from "../app/theme.svelte.js";
</script>

<Toaster
  position="bottom-center"
  theme={theme.value}
  visibleToasts={3}
  closeButton={false}
/>
