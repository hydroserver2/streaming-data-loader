import sys
from pathlib import Path
from logging.config import fileConfig
from platformdirs import user_data_dir
from sqlalchemy import engine_from_config, pool
from alembic import context

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from models import Base  # noqa

config = context.config

if config.config_file_name is not None:
    fileConfig(config.config_file_name)

# Point Alembic at your models for autogenerate support
target_metadata = Base.metadata

# Override the URL with the real platform-specific DB path
DB_DIR = Path(user_data_dir("streaming-data-loader", appauthor=False))
DB_PATH = DB_DIR / "data.db"
DB_URL = f"sqlite:///{DB_PATH}"


def run_migrations_offline() -> None:
    context.configure(
        url=DB_URL,
        target_metadata=target_metadata,
        literal_binds=True,
        dialect_opts={"paramstyle": "named"},
    )
    with context.begin_transaction():
        context.run_migrations()


def run_migrations_online() -> None:
    DB_DIR.mkdir(parents=True, exist_ok=True)
    configuration = config.get_section(config.config_ini_section, {})
    configuration["sqlalchemy.url"] = DB_URL
    connectable = engine_from_config(
        configuration,
        prefix="sqlalchemy.",
        poolclass=pool.NullPool,
    )
    with connectable.connect() as connection:
        context.configure(
            connection=connection,
            target_metadata=target_metadata,
        )
        with context.begin_transaction():
            context.run_migrations()


if context.is_offline_mode():
    run_migrations_offline()
else:
    run_migrations_online()
