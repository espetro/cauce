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
  var numEl = document.getElementById("num");
  var statusEl = document.getElementById("status");
  var metaEl = document.getElementById("meta");
  var resultsEl = document.getElementById("results");
  var tpl = document.getElementById("card-tpl");

  function setStatus(msg) {
    if (!msg) {
      statusEl.hidden = true;
      statusEl.textContent = "";
    } else {
      statusEl.hidden = false;
      statusEl.textContent = msg;
    }
  }

  function setMeta(html) {
    if (!html) {
      metaEl.innerHTML = "";
      return;
    }
    metaEl.innerHTML = html;
  }

  function clearResults() {
    while (resultsEl.firstChild) resultsEl.removeChild(resultsEl.firstChild);
  }

  function renderResults(payload) {
    clearResults();
    var results = (payload && payload.results) || [];
    if (!results.length) {
      setStatus("no results");
      setMeta(
        "<span class='muted'>0 results</span> · request " +
          esc(payload && payload.requestId) +
          " · <a href='/history?q=" + encodeURIComponent(qEl.value) + "'>click history</a>"
      );
      return;
    }
    var source = payload._source || "network";
    var qh = payload._q_hash || "";
    var hits = results.length;
    setMeta(
      "<span class='muted'>" +
        hits +
        " result" +
        (hits === 1 ? "" : "s") +
        " · source: " +
        source +
        " · request " +
        esc(payload.requestId || "") +
        " · <a href='/history?q=" +
        encodeURIComponent(qEl.value) +
        "'>click history</a></span>"
    );
    setStatus("");
    results.forEach(function (r, i) {
      var node = tpl.content.firstElementChild.cloneNode(true);
      var title = r.title || "(untitled)";
      var url = r.url || "";
      var link = node.querySelector(".link");
      link.href = url;
      link.textContent = i + 1 + ". " + title;
      link.setAttribute("data-result-url", url);
      link.setAttribute("data-result-title", title);
      link.setAttribute("data-result-id", r.id || url);
      link.setAttribute("data-query-hash", qh);
      link.addEventListener("click", function () {
        trackClick(link);
      });
      node.setAttribute("data-result-id", r.id || url);
      node.querySelector(".url").textContent = url;
      var metaBits = [];
      if (r.author) metaBits.push(r.author);
      if (r.publishedDate) metaBits.push(r.publishedDate);
      node.querySelector(".meta").textContent = metaBits.join(" · ");
      var txt = (r.text || "").trim();
      if (txt) {
        var p = node.querySelector(".text");
        p.textContent = txt.length > 400 ? txt.slice(0, 399) + "…" : txt;
      } else {
        var te = node.querySelector(".text");
        te.parentNode.removeChild(te);
      }
      var hl = (r.highlights || []).slice(0, 5);
      var hlDetails = node.querySelector(".hl");
      if (hl.length) {
        var ul = hlDetails.querySelector("ul");
        hlDetails.querySelector("summary").textContent =
          "highlights (" + hl.length + ")";
        hl.forEach(function (h) {
          var li = document.createElement("li");
          li.textContent = h;
          ul.appendChild(li);
        });
      } else {
        hlDetails.parentNode.removeChild(hlDetails);
      }
      resultsEl.appendChild(node);
    });
  }

  function esc(s) {
    return String(s == null ? "" : s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
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

  function runSearch(ev) {
    if (ev) ev.preventDefault();
    var q = qEl.value.trim();
    if (!q) return;
    var num = parseInt(numEl.value, 10);
    if (!num || num < 1) num = 10;
    if (num > 30) num = 30;
    setStatus("searching…");
    setMeta("");
    clearResults();
    var url = new URL(window.location.href);
    url.searchParams.set("q", q);
    window.history.replaceState(null, "", url.pathname + "?" + url.searchParams.toString());
    fetch("/search", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        query: q,
        numResults: num,
        contents: { text: true, highlights: true },
      }),
    })
      .then(function (r) {
        if (!r.ok) throw new Error("HTTP " + r.status);
        return r.json();
      })
      .then(function (payload) {
        renderResults(payload);
      })
      .catch(function (err) {
        setStatus("error: " + err.message);
      });
  }

  form.addEventListener("submit", runSearch);

  // auto-run if ?q= present (from a shared link)
  var initial = qEl.value.trim();
  if (initial) runSearch();

  // focus search box on '/'
  document.addEventListener("keydown", function (e) {
    if (e.key === "/" && document.activeElement !== qEl && document.activeElement.tagName !== "INPUT") {
      e.preventDefault();
      qEl.focus();
      qEl.select();
    }
  });
})();
