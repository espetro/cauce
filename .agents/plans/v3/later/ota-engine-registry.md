# later: over-the-air engine registry
Issue: #64

Seam: declarative spec loading (`engines/*.yaml` embedded, `$CAUCE_CONFIG_DIR/engines/`
override) from W1-02.
Trigger: a Bing or Brave selector change breaks users who cannot rebuild; the drift report
(W3-06) has caught at least two such breaks.
Shape: signed `engines.tar.zst` published from this repo's releases, `cauce engine update`
(ETag, signature check, atomic swap), opt-in auto-update interval, health status per spec
from the nightly canary embedded in the bundle so instances route away from broken engines.
Must not change: spec schema without a version bump; embedded defaults remain the fallback.
