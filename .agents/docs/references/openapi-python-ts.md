To keep shipped bundle size down while retaining runtime validation and full bidirectional typing, **Valibot** paired with **`@hey-api/openapi-ts`** (or a split compile-time fetcher) is the ideal stack. Valibot’s modular functional design allows bundlers to tree-shake unused validators, saving up to ~90% bundle size compared to monolithic schema libraries like Zod.

---

### Step 1: Export OpenAPI from FastAPI

Export your schema directly from FastAPI (as outlined before) so your Pydantic definitions serve as the contract:

```bash
python export_openapi.py  # produces openapi.json

```

---

### Step 2: Generate Tree-Shakeable Valibot Schemas

Use `@hey-api/openapi-ts` with the dedicated **Valibot plugin**. This generates modular, functional Valibot schemas (e.g., using `v.object()`, `v.pipe()`, `v.string()`) that tree-shake aggressively in production builds.

1. **Install dependencies:**
```bash
# Production runtime (Valibot conforms to Standard Schema natively)
npm i valibot

# Dev tooling
npm i -D @hey-api/openapi-ts

```


2. **Create configuration (`openapi-ts.config.ts`):**
```typescript
import { defineConfig } from '@hey-api/openapi-ts';

export default defineConfig({
  input: './openapi.json',
  output: './src/api/generated',
  plugins: [
    // Generates standard TS interfaces (zero runtime size)
    '@hey-api/typescript',
    // Generates tree-shakeable Valibot v1 schemas
    {
      name: 'valibot',
      requests: true,     // Schemas for query/path/body requests
      definitions: true,  // Schemas for reusable Pydantic models
    },
  ],
});

```


3. **Run the generator:**
```bash
npx @hey-api/openapi-ts

```



---

### Step 3: Use the Generated Valibot Schemas in React

Because Valibot implements the **Standard Schema** specification, it plugs cleanly into modern form libraries with near-zero glue code.

#### Form Validation with React Hook Form

```tsx
import { useForm } from 'react-hook-form';
import { valibotResolver } from '@hookform/resolvers/valibot';
import { vUserCreate } from '@/api/generated/valibot.gen';
import type { UserCreate } from '@/api/generated/types.gen';

export function RegistrationForm() {
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<UserCreate>({
    resolver: valibotResolver(vUserCreate),
  });

  const onSubmit = (data: UserCreate) => {
    // Strictly typed request body payload
  };

  return (
    <form onSubmit={handleSubmit(onSubmit)}>
      <input {...register('email')} />
      {errors.email && <span>{errors.email.message}</span>}
      <button type="submit">Submit</button>
    </form>
  );
}

```

---

### Step 4: Zero-Weight Network Requests (`openapi-fetch`)

For sending and receiving data without bundling extra runtime validation libraries into your HTTP client, pair the generated types with `openapi-fetch` (~4 kB unpacked, ~1 kB gzipped):

```bash
npm i openapi-fetch

```

```typescript
// src/api/client.ts
import createClient from 'openapi-fetch';
import type { paths } from './generated/types.gen';

export const api = createClient<paths>({ baseUrl: 'http://localhost:8000' });

// Request and Response are strictly typed at compile-time directly against Pydantic
const { data, error } = await api.POST('/api/users', {
  body: {
    name: 'Ada',
    email: 'ada@example.com',
  },
});

```

---

### Summary of Bundle Impact

* **Pydantic (FastAPI):** Validates input automatically on the backend; zero frontend cost.
* **`openapi-fetch` (React):** Handles network calls with full compile-time autocomplete and type safety (~1 kB gzipped).
* **Valibot (React):** Handles user form/UI input validation. Because it imports isolated functions (`pipe`, `string`, `email`), only the exact validators used on that specific route are included in that bundle chunk.
