<!-- MPL-2.0 -->
<script lang="ts"></script>

<section class="compare">
  <div class="wrap">
    <h2>Self-hosted, not pay-per-call.</h2>
    <p class="muted lead">
      Hosted agent-search APIs (Tavily, Exa) sell you searches as a metered
      service. cauce is the other end of the trade: the same three surfaces,
      running where you run.
    </p>
    <table>
      <thead>
        <tr><th></th><th>cauce</th><th>hosted AI-search APIs</th></tr>
      </thead>
      <tbody>
        <tr>
          <th>where it runs</th>
          <td>your machine — loopback by default</td>
          <td class="muted">their cloud, your key</td>
        </tr>
        <tr>
          <th>pricing</th>
          <td>free, MPL-2.0 — your upstream engines are the only limit</td>
          <td class="muted">per-call metering, quotas, keys</td>
        </tr>
        <tr>
          <th>data</th>
          <td>queries and answers stay on your disk</td>
          <td class="muted">queries transit their infra</td>
        </tr>
        <tr>
          <th>surfaces</th>
          <td>UI + API + MCP in one process</td>
          <td class="muted">API-first; you build the rest</td>
        </tr>
        <tr>
          <th>search quality</th>
          <td>inherits whatever your engines return — honest about it</td>
          <td class="muted">tuned proprietary ranking</td>
        </tr>
        <tr>
          <th>pick it when</th>
          <td>privacy, zero ops beyond one process, you own the cache</td>
          <td class="muted">zero-ops by outsourcing, per-call quality</td>
        </tr>
      </tbody>
    </table>
  </div>
</section>

<style>
  .compare { border-top: 1px solid var(--line); }
  h2 { font-size: clamp(26px, 4vw, 36px); margin: 0 0 12px; }
  .lead { max-width: 620px; margin: 0 0 32px; font-size: 17px; }
  table {
    width: 100%;
    border-collapse: collapse;
    border: 1px solid var(--line);
    border-radius: 12px;
    font-size: 15px;
    overflow: hidden;
    display: block;
    overflow-x: auto;
  }
  th, td { padding: 13px 18px; text-align: left; border-bottom: 1px solid var(--line); }
  thead th { border-bottom: 1px solid var(--line); font-size: 13px; color: var(--muted); font-weight: 500; }
  tbody th { font-weight: 500; font-size: 14px; white-space: nowrap; }
  tbody tr:last-child th, tbody tr:last-child td { border-bottom: 0; }
</style>
