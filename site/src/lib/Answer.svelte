<!-- MPL-2.0 -->
<script lang="ts">
  import answerDark from '../assets/answer-dark.png';
</script>

<section class="answers">
  <div class="wrap split">
    <div class="copy">
      <h2>Answers that show their sources.</h2>
      <p class="muted">
        AI mode runs a grounded loop over your own metasearch results — every
        claim carries an inline <code>[n]</code> citation back to a real result
        card, a confidence score, and an <em>ungrounded</em> flag when the
        evidence ran out.
      </p>
      <p class="muted">
        Each finished answer gets a durable URL (<code>/answer/1</code>, not a
        query string) — shareable, linkable, and still there after the 24 h
        cache expires. Yours, in your history, on your disk.
      </p>
      <p class="mono small muted">
        POST /api/answer · SSE stream · works with any OpenAI-compatible provider
      </p>
    </div>
    <figure>
      <img
        src={answerDark}
        alt="A cauce answer page for 'what is reciprocal rank fusion': grounded answer with inline citations, confidence chip, model name, and source cards"
        loading="lazy"
      />
      <figcaption class="muted">A real answer render — dark theme.</figcaption>
    </figure>
  </div>
</section>

<style>
  .answers { border-top: 1px solid var(--line); }
  .split {
    display: grid;
    grid-template-columns: minmax(0, 5fr) minmax(0, 7fr);
    gap: 48px;
    align-items: start;
  }
  h2 { font-size: clamp(26px, 4vw, 36px); margin: 0 0 20px; }
  .copy p { margin: 0 0 16px; }
  .small { font-size: 13px; }
  figure {
    margin: 0;
    border: 1px solid var(--line);
    border-radius: 12px;
    overflow: hidden;
    box-shadow: 0 8px 30px rgba(0, 0, 0, 0.12);
  }
  figcaption {
    padding: 10px 16px;
    font-size: 13px;
    border-top: 1px solid var(--line);
    background: var(--card-bg);
  }
  figure img { display: block; }
  @media (max-width: 820px) {
    .split { grid-template-columns: 1fr; gap: 32px; }
  }
</style>
