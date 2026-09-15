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
from dataclasses import dataclass
from pathlib import Path

try:
    import tomllib
except ImportError:  # py3.10
    import tomli as tomllib  # type: ignore[no-redef]

log = logging.getLogger(__name__)

VALID_PROVIDERS = {"openai", "anthropic", "groq", "mistral", "ollama", "huggingface"}


@dataclass
class AIConfig:
    provider: str
    model: str
    api_key: str | None = None
    api_key_env: str | None = None
    base_url: str | None = None
    enabled: bool = True

    def resolve_api_key(self) -> str | None:
        """Explicit key wins, then the named env var."""
        if self.api_key:
            return self.api_key
        if self.api_key_env:
            return os.environ.get(self.api_key_env)
        return None

    def to_toml(self) -> str:
        """Serialize back to a config.toml [ai] section."""
        lines = ["[ai]", f'provider = "{self.provider}"', f'model = "{self.model}"']
        if self.api_key:
            lines.append(f'api_key = "{self.api_key}"')
        if self.api_key_env:
            lines.append(f'api_key_env = "{self.api_key_env}"')
        if self.base_url:
            lines.append(f'base_url = "{self.base_url}"')
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


_ENV_RE = re.compile(r"^\{env\.([A-Za-z_][A-Za-z0-9_]*)\}$")


def load_dotenv(path: Path | None = None) -> None:
    """Load KEY=VALUE pairs from a .env file (cwd default) into os.environ.

    Existing environment variables win. No quoting gymnastics: values are
    taken verbatim after the first '='.
    """
    p = path or Path(".env")
    if not p.is_file():
        return
    for line in p.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        key, value = key.strip(), value.strip()
        if key and key not in os.environ:
            os.environ[key] = value


def _resolve_env(value: str, path: Path, key: str) -> str:
    """Resolve '{env.NAME}' interpolation against os.environ."""
    m = _ENV_RE.match(value.strip())
    if not m:
        return value
    name = m.group(1)
    resolved = os.environ.get(name)
    if resolved is None:
        raise ConfigError(f"{path}: [ai] {key}: environment variable {name!r} is not set")
    return resolved


def config_dir() -> Path:
    d = os.getenv("OXE_CONFIG_DIR", os.path.expanduser("~/.config/oxe"))
    return Path(d)


def config_path() -> Path:
    return config_dir() / "config.toml"


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
        with open(p, "rb") as f:
            data = tomllib.load(f)
    except tomllib.TOMLDecodeError as e:
        raise ConfigError(f"{p}: invalid TOML: {e}") from e

    if not isinstance(data, dict):
        raise ConfigError(f"{p}: expected top-level tables")
    ai = data.get("ai")
    if ai is None:
        return None
    if not isinstance(ai, dict):
        raise ConfigError(f"{p}: [ai] must be a table, got {type(ai).__name__}")

    unknown = set(ai) - {
        "provider", "model", "api_key", "api_key_env", "base_url", "enabled",
    }
    if unknown:
        raise ConfigError(f"{p}: unknown [ai] keys: {', '.join(sorted(unknown))}")

    provider = ai.get("provider")
    model = ai.get("model")
    if not isinstance(provider, str) or not provider.strip():
        raise ConfigError(f"{p}: [ai] provider must be a non-empty string")
    if not isinstance(model, str) or not model.strip():
        raise ConfigError(f"{p}: [ai] model must be a non-empty string")
    provider, model = provider.strip(), model.strip()

    if provider not in VALID_PROVIDERS:
        raise ConfigError(
            f"{p}: [ai] provider {provider!r} not supported; "
            f"expected one of {', '.join(sorted(VALID_PROVIDERS))}"
        )

    for key in ("api_key", "api_key_env", "base_url"):
        v = ai.get(key)
        if v is not None and not isinstance(v, str):
            raise ConfigError(f"{p}: [ai] {key} must be a string")
        if isinstance(v, str):
            ai[key] = _resolve_env(v, p, key)
    enabled = ai.get("enabled", True)
    if not isinstance(enabled, bool):
        raise ConfigError(f"{p}: [ai] enabled must be a boolean")

    cfg = AIConfig(
        provider=provider,
        model=model,
        api_key=ai.get("api_key"),
        api_key_env=ai.get("api_key_env"),
        base_url=ai.get("base_url"),
        enabled=enabled,
    )
    if not enabled:
        return None
    return cfg
