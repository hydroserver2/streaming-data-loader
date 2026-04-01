from datetime import datetime
from typing import Optional
from sqlalchemy import (
    Boolean, DateTime, ForeignKey,
    Integer, JSON, String, Text
)
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column, relationship


# To update models:
# alembic revision --autogenerate -m "description of change"


class Base(DeclarativeBase):
    pass


class HydroServerConnection(Base):
    __tablename__ = "hydroserver_connection"

    id: Mapped[str] = mapped_column(String, primary_key=True)
    name: Mapped[str] = mapped_column(String(255), nullable=False)
    host: Mapped[str] = mapped_column(String(255), nullable=False)
    auth_type: Mapped[str] = mapped_column(String(50), nullable=False)  # "apikey" | "userpass"
    api_key: Mapped[Optional[str]] = mapped_column(String, nullable=True)
    username: Mapped[Optional[str]] = mapped_column(String(255), nullable=True)
    password: Mapped[Optional[str]] = mapped_column(String, nullable=True)

    tasks: Mapped[list["Task"]] = relationship(back_populates="connection")

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "name": self.name,
            "host": self.host,
            "auth_type": self.auth_type,
            "api_key": self.api_key,
            "username": self.username,
            "password": self.password,
        }


class Task(Base):
    __tablename__ = "task"

    id: Mapped[str] = mapped_column(String, primary_key=True)
    name: Mapped[str] = mapped_column(String(255), nullable=False)
    connection_id: Mapped[str] = mapped_column(
        String, ForeignKey("hydroserver_connection.id"), nullable=False
    )
    schedule: Mapped[Optional[dict]] = mapped_column(JSON, nullable=True)
    is_active: Mapped[bool] = mapped_column(Boolean, default=True, nullable=False)
    source_type: Mapped[str] = mapped_column(String(50), nullable=False)  # "http" | "local"
    file_path: Mapped[str] = mapped_column(Text, nullable=False)
    csv_delimiter: Mapped[str] = mapped_column(String(10), default=",", nullable=False)
    csv_header_row: Mapped[int] = mapped_column(Integer, default=0, nullable=False)
    csv_timestamp_column: Mapped[str] = mapped_column(String(255), nullable=False)
    csv_timestamp_format: Mapped[str] = mapped_column(String(255), nullable=False)
    column_mappings: Mapped[Optional[list]] = mapped_column(JSON, nullable=True)

    connection: Mapped["HydroServerConnection"] = relationship(back_populates="tasks")
    runs: Mapped[list["TaskRun"]] = relationship(back_populates="task")

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "name": self.name,
            "connection_id": self.connection_id,
            "schedule": self.schedule,
            "is_active": self.is_active,
            "source_type": self.source_type,
            "file_path": self.file_path,
            "csv_delimiter": self.csv_delimiter,
            "csv_header_row": self.csv_header_row,
            "csv_timestamp_column": self.csv_timestamp_column,
            "csv_timestamp_format": self.csv_timestamp_format,
            "column_mappings": self.column_mappings,
        }


class TaskRun(Base):
    __tablename__ = "task_run"

    id: Mapped[str] = mapped_column(String, primary_key=True)
    task_id: Mapped[str] = mapped_column(
        String, ForeignKey("task.id"), nullable=False
    )
    status: Mapped[str] = mapped_column(String(50), nullable=False)  # "started" | "success" | "failure"
    started_at: Mapped[datetime] = mapped_column(DateTime, nullable=False)
    completed_at: Mapped[Optional[datetime]] = mapped_column(DateTime, nullable=True)
    error_message: Mapped[Optional[str]] = mapped_column(Text, nullable=True)
    success_count: Mapped[Optional[int]] = mapped_column(Integer, nullable=True)
    failure_count: Mapped[Optional[int]] = mapped_column(Integer, nullable=True)
    skipped_count: Mapped[Optional[int]] = mapped_column(Integer, nullable=True)
    values_loaded_total: Mapped[Optional[int]] = mapped_column(Integer, nullable=True)
    earliest_timestamp: Mapped[Optional[datetime]] = mapped_column(DateTime, nullable=True)
    latest_timestamp: Mapped[Optional[datetime]] = mapped_column(DateTime, nullable=True)

    task: Mapped["Task"] = relationship(back_populates="runs")

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "task_id": self.task_id,
            "status": self.status,
            "started_at": self.started_at.isoformat() if self.started_at else None,
            "completed_at": self.completed_at.isoformat() if self.completed_at else None,
            "error_message": self.error_message,
            "success_count": self.success_count,
            "failure_count": self.failure_count,
            "skipped_count": self.skipped_count,
            "values_loaded_total": self.values_loaded_total,
            "earliest_timestamp": self.earliest_timestamp.isoformat() if self.earliest_timestamp else None,
            "latest_timestamp": self.latest_timestamp.isoformat() if self.latest_timestamp else None,
        }
