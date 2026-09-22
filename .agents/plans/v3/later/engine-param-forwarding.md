# later: engine-param forwarding (safesearch/time_range) + parse.allow_empty
Issue: #122

Seam: declarative spec templates (`{safesearch}`/`{time_range}` vars) + exec protocol v2
fields (related: #88).
Trigger: a spec or exec engine that can actually act on them (Bing/Brave both support
freshness/safe params upstream). Today both params are validated and cache-keyed but
never reach engines — honest gap.
Shape: safesearch/time_range reach engines through spec templating and protocol v2; also
carries a `parse.allow_empty: true` spec knob for engines where an empty page is
legitimate (currently any zero-match is `Parse`, which makes a genuine end-of-results
page a spurious failure).
