To pack a single README with deep technical context while keeping it clean and skimmable for everyday users, use a **Progressive Disclosure & LLM-Context Pattern**.

This balances human visual scanning with machine readability using native Markdown and GitHub-Flavored Markdown (GFM).

---

### 1. Structural Strategy: The "Inverted Pyramid"

Organize your README so that critical onboarding details sit at the top, and machine/deep-dive details sit at the bottom.

* **Layer 1: The Skim Zone (Top 20%)**
* One-line pitch, core features, quick visual (diagram/demo gif), and a copy-pasteable 30-second quickstart.


* **Layer 2: Daily Operations (Next 30%)**
* Essential configuration, basic usage examples, common CLI flags.


* **Layer 3: Advanced Deep Dive (Next 30%)**
* Edge cases, internal architecture, performance tuning, and complex configurations wrapped in disclosure widgets.


* **Layer 4: Machine Context & Reference (Bottom 20%)**
* Full schema specifications, comprehensive glossary, LLM instructions, and troubleshooting index.



---

### 2. Formatting Patterns for Mixed Audiences

#### Native Collapsible Sections (`<details>` / `<summary>`)

Use native HTML disclosure widgets for deep-dive content. Humans only expand them when needed, but **LLMs parse raw Markdown and read the text inside collapsed blocks just as easily as the rest of the document**.

```markdown
### Installation

Run the standard install:
```bash
npm install my-tool

```

If you are deploying in an air-gapped environment or use an internal artifactory:

1. Export the environment variable `MY_TOOL_REGISTRY=https://artifactory.local`.
2. Ensure CA certs are mounted at `/etc/ssl/certs`.

#### Markdown Tables for Dense Data

Tables provide compact reference material for humans and strong tabular signal for LLMs to map key-value pairs.

| Env Variable | Type | Default | Description / LLM Hint |
| --- | --- | --- | --- |
| `CACHE_TTL` | Integer | `3600` | Duration in seconds before cache invalidation. |
| `STRICT_SSL` | Boolean | `true` | Set to `false` only in test suites; never in prod. |

#### Semantic Code Blocks

Always tag code blocks with explicit languages (`json`, `yaml`, `bash`, `ts`). For configuration templates, annotate options directly with inline comments so an LLM understands *why* a flag exists:

```yaml
# config.yaml
server:
  port: 8080
  # LLM/Edge case: Enable if running behind Cloudflare or AWS ALB
  trust_proxy_headers: true

```

---

### 3. Dedicated LLM & Agent Context Section

Add an explicit machine-readable section near the bottom. LLMs looking for guidance (or developers loading your README into an LLM context window) can anchor directly to this block.

```markdown
---

## Machine Context & LLM Usage

> **Context for AI coding assistants (Copilot, Claude, Cursor):**
> 
> * **Project Role:** High-throughput streaming proxy written in Go.
> * **Key Invariants:** Never mutate incoming payload headers directly; clone the buffer first.
> * **Design Patterns:** Uses the Actor pattern via Go channels. Avoid introducing mutexes unless profile-guided.
> * **Testing:** Mock HTTP clients via `internal/testutil/mock.go`. Do not write network-dependent integration tests in unit test suites.

```

---

### 4. Layout Best Practices

* **Anchor Jump Links:** Place a compact **Table of Contents** immediately after your quickstart to let power users jump straight to the advanced sections.
* **Visual Anchors:** Use standalone bold labels or blockquotes (`>`) to highlight security caveats, deprecation notices, or performance limits.
* **Complementary `CONTRIBUTING.md` / `ARCHITECTURE.md`:** If single-file size pushes beyond **1,500–2,000 lines**, consider splitting only the deepest internal architecture into an `ARCHITECTURE.md` file in the same repository. Both humans and LLMs can easily follow relative Markdown links (`[Architecture Overview](./ARCHITECTURE.md)`).
