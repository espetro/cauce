"""LLM configuration loader. Config file is the source of truth.

Location: ``$OXE_CONFIG_DIR/config.toml`` (default ``~/.config/oxe``).

Schema::

    [ai]
    enabled = true            # optional, default true when model set
    provider = "openai"       # openai | anthropic | any aisuite provider
    model = "gpt-4o-mini"
    api_key = "sk-..."        # optional; or env var name via api_key_env
    api_key_env = "OPENAI_API_KEY"  # optional, read from environment
    base_url = "https://..."  # optional override

AI mode is OFF unless both provider and model are configured.
"""

import logging
import os
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import cast

import tomllib

log = logging.getLogger(__name__)

VALID_PROVIDERS = frozenset({"openai", "anthropic", "groq", "mistral", "ollama", "huggingface"})

# Tool-loop cap for the AI answer pipeline (oxe.ai.stream_answer): the model
# gets at most this many provider rounds to either answer or stop calling tools.
MAX_ITERATIONS = 5
# Answers below this confidence are streamed to the user but never cached
# (low-confidence / greeting replies poisoning the cache for 24h).
CACHE_MIN_CONFIDENCE = 4

_KNOWN_AI_KEYS = frozenset({"provider", "model", "api_key", "api_key_env", "base_url", "enabled"})
_ENV_RE = re.compile(r"^\{env\.([A-Za-z_][A-Za-z0-9_]*)\}$")


@dataclass(frozen=True)
class AIConfig:
    provider: str
    model: str
    api_key: str | None = None
    api_key_env: str | None = None
    base_url: str | None = None
    enabled: bool = True
    # Raw ``{env.NAME}`` templates for fields that came from config.toml with
    # interpolation, keyed by field name. Kept so save_config writes the
    # template back instead of flattening resolved secrets to plaintext.
    env_templates: dict[str, str] = field(default_factory=dict)

    def resolve_api_key(self) -> str | None:
        """Explicit key wins, then the named env var."""
        if self.api_key:
            return self.api_key
        if self.api_key_env:
            return os.environ.get(self.api_key_env)
        return None

    def to_toml(self) -> str:
        """Serialize back to a config.toml [ai] section.

        Fields that were loaded from ``{env.NAME}`` interpolation and not
        explicitly replaced keep their template string, so resolved secrets
        never land in config.toml.
        """

        def out(name: str, value: str | None) -> str:
            v = self.env_templates.get(name, value)
            return f'{name} = "{v}"' if v else ""

        lines = ["[ai]", f'provider = "{self.provider}"', f'model = "{self.model}"']
        for name, value in (
            ("api_key", self.api_key),
            ("api_key_env", self.api_key_env),
            ("base_url", self.base_url),
        ):
            line = out(name, value)
            if line:
                lines.append(line)
        lines.append(f"enabled = {str(self.enabled).lower()}")
        return "\n".join(lines) + "\n"


def save_config(cfg: AIConfig, path: Path | None = None) -> Path:
    """Write [ai] section to config.toml, creating the dir if needed."""
    p = path or config_path()
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(cfg.to_toml())
    return p


class ConfigError(ValueError):
    """Raised for malformed config files with a user-facing message."""


def load_dotenv() -> None:
    """Load KEY=VALUE pairs from a .env file (cwd, then config dir) into os.environ.

    Existing environment variables win. No quoting gymnastics: values are
    taken verbatim after the first '='.
    """
    try:
        candidates = [Path(".env"), config_dir() / ".env"]
        for p in candidates:
            if p.is_file():
                _load_dotenv_file(p)
                break
    except OSError:
        log.debug("load_dotenv: no readable .env candidate")


def _load_dotenv_file(p: Path) -> None:
    for raw_line in p.read_text().splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        key, value = key.strip(), value.strip()
        if key and key not in os.environ:
            os.environ[key] = value


def _resolve_env(value: str, path: Path, key: str) -> tuple[str, str | None]:
    """Resolve '{env.NAME}' interpolation; return (resolved, template|None)."""
    m = _ENV_RE.match(value.strip())
    if not m:
        return value, None
    name = m.group(1)
    resolved = os.environ.get(name)
    if resolved is None:
        msg = f"{path}: [ai] {key}: environment variable {name!r} is not set"
        raise ConfigError(msg)
    return resolved, value.strip()


def config_dir() -> Path:
    d = os.getenv("OXE_CONFIG_DIR", str(Path.home() / ".config" / "oxe"))
    return Path(d)


def config_path() -> Path:
    return config_dir() / "config.toml"


def cache_dir() -> Path:
    """``OXE_CACHE_DIR``, defaulting to ``~/.cache/oxe`` (same default as legacy
    ``oxe/__main__.py``'s ``_default_db``). ``tests/conftest.py`` redirects this
    env var to a tmp path before any ``oxe`` module import."""
    d = os.getenv("OXE_CACHE_DIR", str(Path.home() / ".cache" / "oxe"))
    return Path(d)


def cache_db_path() -> Path:
    return cache_dir() / "cache.db"


def _as_object_dict(value: object, ctx: str) -> dict[str, object]:
    """Narrow an arbitrary (TOML-decoded) value into a str-keyed dict, or raise.

    ``tomllib.load`` is typed as returning ``dict[str, Any]`` upstream, so
    this is the one place that ``Any`` from that boundary gets converted into
    a concrete, parameterised type via explicit ``isinstance`` narrowing.
    """
    if not isinstance(value, dict):
        msg = f"{ctx}: expected a table, got {type(value).__name__}"
        raise ConfigError(msg)
    raw = cast(dict[object, object], value)
    out: dict[str, object] = {}
    for k, v in raw.items():
        if not isinstance(k, str):
            msg = f"{ctx}: non-string key {k!r}"
            raise ConfigError(msg)
        out[k] = v
    return out


def _require_str(ai: dict[str, object], key: str, path: Path) -> str:
    v = ai.get(key)
    if not isinstance(v, str) or not v.strip():
        msg = f"{path}: [ai] {key} must be a non-empty string"
        raise ConfigError(msg)
    return v.strip()


def _optional_str(ai: dict[str, object], key: str, path: Path) -> str | None:
    v = ai.get(key)
    if v is None:
        return None
    if not isinstance(v, str):
        msg = f"{path}: [ai] {key} must be a string"
        raise ConfigError(msg)
    return v


def _optional_bool(ai: dict[str, object], key: str, path: Path, *, default: bool) -> bool:
    v = ai.get(key, default)
    if not isinstance(v, bool):
        msg = f"{path}: [ai] {key} must be a boolean"
        raise ConfigError(msg)
    return v


def _interpolate_fields(
    ai: dict[str, object], path: Path
) -> tuple[dict[str, str | None], dict[str, str]]:
    """Resolve ``{env.NAME}`` templates for api_key/api_key_env/base_url."""
    resolved: dict[str, str | None] = {}
    templates: dict[str, str] = {}
    for key in ("api_key", "api_key_env", "base_url"):
        value = _optional_str(ai, key, path)
        if value is None:
            resolved[key] = None
            continue
        value_resolved, template = _resolve_env(value, path, key)
        resolved[key] = value_resolved
        if template:
            templates[key] = template
    return resolved, templates


def load_config(path: Path | None = None) -> AIConfig | None:
    """Load [ai] section from config.toml. None means AI mode is OFF.

    String values may reference environment variables with ``{env.NAME}``
    (e.g. ``api_key = "{env.OXE_AI_API_KEY}"``); a missing variable is a
    ConfigError. A .env file in the working directory is loaded first
    (existing environment variables win).

    Raises ConfigError with a clear message for malformed files; a missing
    file is not an error (AI simply unconfigured).
    """
    load_dotenv()
    p = path or config_path()
    if not p.is_file():
        return None
    try:
        with p.open("rb") as f:
            raw = cast(object, tomllib.load(f))
    except tomllib.TOMLDecodeError as e:
        msg = f"{p}: invalid TOML: {e}"
        raise ConfigError(msg) from e

    data = _as_object_dict(raw, str(p))
    ai_raw = data.get("ai")
    if ai_raw is None:
        return None
    ai = _as_object_dict(ai_raw, f"{p}: [ai]")

    unknown = set(ai) - _KNOWN_AI_KEYS
    if unknown:
        msg = f"{p}: unknown [ai] keys: {', '.join(sorted(unknown))}"
        raise ConfigError(msg)

    provider = _require_str(ai, "provider", p)
    model = _require_str(ai, "model", p)
    if provider not in VALID_PROVIDERS:
        msg = (
            f"{p}: [ai] provider {provider!r} not supported; "
            f"expected one of {', '.join(sorted(VALID_PROVIDERS))}"
        )
        raise ConfigError(msg)

    resolved, templates = _interpolate_fields(ai, p)
    enabled = _optional_bool(ai, "enabled", p, default=True)

    cfg = AIConfig(
        provider=provider,
        model=model,
        api_key=resolved["api_key"],
        api_key_env=resolved["api_key_env"],
        base_url=resolved["base_url"],
        enabled=enabled,
        env_templates=templates,
    )
    if not enabled:
        return None
    return cfg
