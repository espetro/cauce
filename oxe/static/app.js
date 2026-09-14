(function () {
  "use strict";

  // -- delete confirmation (cache + history) -----------------------------
  document.querySelectorAll("form.danger[data-confirm]").forEach(function (f) {
    f.addEventListener("submit", function (e) {
      if (!window.confirm(f.getAttribute("data-confirm"))) e.preventDefault();
    });
  });

  // -- search form -------------------------------------------------------
  var form = document.getElementById("search-form");
  if (!form) return;
  var qEl = document.getElementById("q");
  var statusEl = document.getElementById("status");
  var metaEl = document.getElementById("meta");
  var metaLineEl = document.getElementById("meta-line");
  var shareEl = document.getElementById("share");
  var aiEl = document.getElementById("ai-stub");
  var aiBtn = document.getElementById("ai-btn");
  var aiNote = document.getElementById("ai-note");
  var pagerEl = document.getElementById("pager");
  var resultsEl = document.getElementById("results");
  var tpl = document.getElementById("card-tpl");
  var submitBtn = form.querySelector("button[type=submit]");
  var PAGE_SIZE = 10;

  function setStatus(msg) {
    if (!msg) {
      statusEl.hidden = true;
      statusEl.textContent = "";
    } else {
      statusEl.hidden = false;
      statusEl.textContent = msg;
    }
  }

  function showChrome(show) {
    metaEl.hidden = !show;
    shareEl.hidden = !show;
    aiEl.hidden = false; // reserved slot, always present on results view
    if (show) aiEl.hidden = false;
  }

  function clearResults() {
    while (resultsEl.firstChild) resultsEl.removeChild(resultsEl.firstChild);
  }

  function fmtDur(s) {
    if (s == null || s < 0) return "0s";
    if (s < 60) return s + "s";
    if (s < 3600) return Math.floor(s / 60) + "m";
    if (s < 86400) return Math.floor(s / 3600) + "h";
    return Math.floor(s / 86400) + "d";
  }

  function fmtAge(ageS) {
    return ageS < 60 ? "just now" : fmtDur(ageS);
  }

  function fmtTtlLeft(ttlS) {
    if (ttlS <= 0) return "expired";
    return fmtDur(ttlS) + " left";
  }

  function metaText(payload, share) {
    var results = (payload && payload.results) || [];
    var bits = [results.length + " result" + (results.length === 1 ? "" : "s")];
    var source = (share && share.source) || (payload && payload._source);
    if (source) bits.push("from " + source);
    var age = share && share.age_s;
    if (age) bits.push(fmtAge(age) + " old");
    var ttl = share && share.ttl_left_s;
    if (ttl != null) bits.push("ttl " + fmtTtlLeft(ttl));
    return bits.join(" - ");
  }

  function setMeta(text) {
    metaLineEl.textContent = text;
    metaEl.hidden = !text;
    shareEl.hidden = !text;
  }

  function esc(s) {
    return String(s == null ? "" : s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#39;");
  }

  function domainOf(url) {
    try {
      return new URL(url).hostname.replace(/^www\./, "") || url;
    } catch (_) {
      return url;
    }
  }

  function buildResult(r) {
    var node = tpl.content.firstElementChild.cloneNode(true);
    var title = r.title || "(untitled)";
    var url = r.url || "";
    var domain = domainOf(url);
    node.setAttribute("data-result-id", r.id || url);
    var fav = node.querySelector(".fav");
    if (domain && url) {
      fav.src = "https://icons.duckduckgo.com/ip3/" + encodeURIComponent(domain) + ".ico";
      fav.onerror = function () {
        fav.style.display = "none";
      };
    } else {
      fav.style.display = "none";
    }
    node.querySelector(".domain").textContent = domain;
    var link = node.querySelector(".link");
    link.href = url;
    link.textContent = title;
    link.setAttribute("data-result-url", url);
    link.setAttribute("data-result-title", title);
    link.setAttribute("data-result-id", r.id || url);
    link.setAttribute("data-query-hash", (r._q_hash || currentQueryHash || ""));
    link.addEventListener("click", function () {
      trackClick(link);
    });
    var snippet = (r.text || "").trim();
    var sn = node.querySelector(".snippet");
    if (snippet) {
      sn.textContent = snippet.length > 200 ? snippet.slice(0, 199) + "…" : snippet;
    } else {
      sn.parentNode.removeChild(sn);
    }
    var details = node.querySelector(".preview");
    if (snippet) {
      details.querySelector(".preview-text").textContent =
        snippet.length > 400 ? snippet.slice(0, 400) : snippet;
    } else {
      details.parentNode.removeChild(details);
    }
    return node;
  }

  var lastShare = null;

  function renderResults(payload, share) {
    lastShare = share || null;
    var results = (payload && payload.results) || [];
    if (!results.length) {
      setMeta(metaText(payload, lastShare));
      clearResults();
      setStatus("no results");
      hidePager();
      return;
    }
    setStatus("");
    setMeta(metaText(payload, lastShare));
    clearResults();
    var frag = document.createDocumentFragment();
    results.forEach(function (r) {
      frag.appendChild(buildResult(r));
    });
    resultsEl.appendChild(frag);
    renderPager(payload, results.length);
  }

  function hidePager() {
    pagerEl.hidden = true;
    pagerEl.textContent = "";
  }

  function renderPager(payload, nResults) {
    hidePager();
    var page = Number(payload && payload._page) || currentPage || 1;
    var total = Number(payload && payload._total_pages) || 0;
    if (!total && nResults === PAGE_SIZE) total = page + 1; // maybe more
    if (total <= 1) return;
    var q = qEl.value.trim();
    var html = "";
    if (page > 1) html += "<a href='/search?q=" + encodeURIComponent(q) + "&p=" + (page - 1) + "' data-p='" + (page - 1) + "' class='pg-prev'>previous</a> ";
    html += "<span class='pg-label'>page " + page + " of " + total + "</span>";
    if (page < total) html += " <a href='/search?q=" + encodeURIComponent(q) + "&p=" + (page + 1) + "' data-p='" + (page + 1) + "' class='pg-next'>next &gt;</a>";
    pagerEl.innerHTML = html;
    pagerEl.hidden = false;
    pagerEl.querySelectorAll("a").forEach(function (a) {
      a.addEventListener("click", function (ev) {
        ev.preventDefault();
        runSearch(null, q, Number(a.getAttribute("data-p")));
      });
    });
  }

  function trackClick(link) {
    if (!navigator.sendBeacon) return;
    var body = JSON.stringify({
      query_hash: link.getAttribute("data-query-hash") || "",
      result_id: link.getAttribute("data-result-id") || "",
      url: link.getAttribute("data-result-url") || "",
      title: link.getAttribute("data-result-title") || "",
      source: "web",
    });
    try {
      navigator.sendBeacon("/click", new Blob([body], { type: "application/json" }));
    } catch (_) {
      /* best effort */
    }
  }

  var currentPage = 1;
  var currentQueryHash = "";

  function runSearch(ev, queryOverride, page) {
    if (ev) ev.preventDefault();
    var q = (queryOverride != null ? queryOverride : qEl.value).trim();
    if (!q) return;
    if (page == null) page = 1;
    currentPage = page;
    var btn = submitBtn;
    btn.disabled = true;
    var old = btn.textContent;
    btn.textContent = "searching…";
    fetch("/search", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        query: q,
        numResults: PAGE_SIZE,
        contents: { text: true, highlights: true },
      }),
    })
      .then(function (r) {
        if (!r.ok) throw new Error("HTTP " + r.status);
        return r.json();
      })
      .then(function (payload) {
        btn.disabled = false;
        btn.textContent = old;
        currentQueryHash = payload._q_hash || "";
        var url = new URL("/search", window.location.origin);
        url.searchParams.set("q", q);
        if (page > 1) url.searchParams.set("p", String(page));
        window.history.pushState(null, "", url.pathname + url.search);
        renderResults(payload, payload && payload._share);
      })
      .catch(function () {
        btn.disabled = false;
        btn.textContent = old;
        // previous results left intact
        setStatus("error: backend rate limited, retry in a moment");
      });
  }

  form.addEventListener("submit", runSearch);

  window.addEventListener("popstate", function () {
    var u = new URL(window.location.href);
    var q = u.searchParams.get("q");
    if (q) {
      qEl.value = q;
      runSearch(null, q, Number(u.searchParams.get("p")) || 1);
    }
  });

  // -- share row ----------------------------------------------------------
  function flash(btn, orig) {
    btn.textContent = "copied";
    btn.disabled = true;
    setTimeout(function () {
      btn.textContent = orig;
      btn.disabled = false;
    }, 1000);
  }

  var copyLinkBtn = document.getElementById("copy-link");
  copyLinkBtn.addEventListener("click", function () {
    var q = qEl.value.trim();
    var url = window.location.origin + "/search?q=" + encodeURIComponent(q);
    navigator.clipboard.writeText(url).then(function () {
      flash(copyLinkBtn, "copy link");
    });
  });

  var copyJsonBtn = document.getElementById("copy-json");
  copyJsonBtn.addEventListener("click", function () {
    var q = qEl.value.trim();
    fetch("/search?q=" + encodeURIComponent(q), {
      headers: { Accept: "application/json" },
    })
      .then(function (r) {
        return r.text();
      })
      .then(function (body) {
        return navigator.clipboard.writeText(body);
      })
      .then(function () {
        flash(copyJsonBtn, "copy json");
      });
  });

  // -- ai stub: never auto-runs -------------------------------------------
  aiBtn.addEventListener("click", function () {
    aiNote.hidden = false;
  });

  // auto-run if ?q= present (from a shared link), unless results are already
  // server-rendered on the page
  var initial = qEl.value.trim();
  showChrome(!!(initial && resultsEl.firstElementChild));
  if (initial && !resultsEl.firstElementChild) runSearch(null, initial);
  if (initial) aiEl.hidden = false;

  // focus search box on '/'
  document.addEventListener("keydown", function (e) {
    if (e.key === "/" && document.activeElement !== qEl && document.activeElement.tagName !== "INPUT") {
      e.preventDefault();
      qEl.focus();
      qEl.select();
    }
  });
})();
