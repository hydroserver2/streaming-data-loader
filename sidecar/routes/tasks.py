import uuid
from flask import Blueprint, jsonify, request
from sqlalchemy.orm import joinedload
from database import get_session
from models import Task, TaskRun
from scheduler import register_task, remove_task, execute_task

bp = Blueprint("tasks", __name__, url_prefix="/tasks")


@bp.get("/")
def list_tasks():
    with get_session() as session:
        tasks = (
            session.query(Task)
            .options(joinedload(Task.runs))
            .all()
        )
        result = []
        for task in tasks:
            d = task.to_dict()
            latest_run = (
                sorted(task.runs, key=lambda r: r.started_at, reverse=True)[0]
                if task.runs else None
            )
            d["latest_run"] = latest_run.to_dict() if latest_run else None
            result.append(d)
        return jsonify(result)


@bp.get("/<string:task_id>")
def get_task(task_id: str):
    with get_session() as session:
        task = session.get(Task, task_id)
        if not task:
            return jsonify({"error": "Task not found"}), 404
        return jsonify(task.to_dict())


@bp.post("/")
def create_task():
    data = request.get_json()
    if not data:
        return jsonify({"error": "No data provided"}), 400

    error = _validate_task(data)
    if error:
        return jsonify({"error": error}), 422

    with get_session() as session:
        from models import HydroServerConnection
        if not session.get(HydroServerConnection, data["connection_id"]):
            return jsonify({"error": "Connection not found"}), 404

        task = Task(
            id=str(uuid.uuid4()),
            name=data["name"],
            connection_id=data["connection_id"],
            schedule=data.get("schedule"),
            is_active=data.get("is_active", True),
            source_type=data["source_type"],
            file_path=data["file_path"],
            csv_delimiter=data.get("csv_delimiter", ","),
            csv_header_row=data.get("csv_header_row", 0),
            csv_timestamp_column=data["csv_timestamp_column"],
            csv_timestamp_format=data["csv_timestamp_format"],
            column_mappings=data.get("column_mappings", []),
        )
        session.add(task)
        session.commit()
        session.refresh(task)
        task_dict = task.to_dict()

    register_task(task_dict)
    return jsonify(task_dict), 201


@bp.put("/<string:task_id>")
def update_task(task_id: str):
    data = request.get_json()
    if not data:
        return jsonify({"error": "No data provided"}), 400

    error = _validate_task(data)
    if error:
        return jsonify({"error": error}), 422

    with get_session() as session:
        task = session.get(Task, task_id)
        if not task:
            return jsonify({"error": "Task not found"}), 404

        task.name = data["name"]
        task.connection_id = data["connection_id"]
        task.schedule = data.get("schedule")
        task.is_active = data.get("is_active", True)
        task.source_type = data["source_type"]
        task.file_path = data["file_path"]
        task.csv_delimiter = data.get("csv_delimiter", ",")
        task.csv_header_row = data.get("csv_header_row", 0)
        task.csv_timestamp_column = data["csv_timestamp_column"]
        task.csv_timestamp_format = data["csv_timestamp_format"]
        task.column_mappings = data.get("column_mappings", [])

        session.commit()
        session.refresh(task)
        task_dict = task.to_dict()

    register_task(task_dict)
    return jsonify(task_dict)


@bp.delete("/<string:task_id>")
def delete_task(task_id: str):
    with get_session() as session:
        task = session.get(Task, task_id)
        if not task:
            return jsonify({"error": "Task not found"}), 404
        session.delete(task)
        session.commit()

    remove_task(task_id)
    return jsonify({"ok": True})


@bp.post("/<string:task_id>/run")
def run_task_now(task_id: str):
    """Trigger an immediate out-of-schedule run."""
    with get_session() as session:
        task = session.get(Task, task_id)
        if not task:
            return jsonify({"error": "Task not found"}), 404

    # Run in the scheduler's thread pool so it doesn't block the request
    from apscheduler.triggers.date import DateTrigger
    from datetime import datetime
    from scheduler import scheduler
    scheduler.add_job(
        execute_task,
        trigger=DateTrigger(run_date=datetime.utcnow()),
        args=[task_id],
        id=f"manual_{task_id}_{uuid.uuid4().hex[:8]}",
    )
    return jsonify({"ok": True, "message": "Run triggered"})


def _validate_task(data: dict) -> str | None:
    if not data.get("name"):
        return "name is required"
    if not data.get("connection_id"):
        return "connection_id is required"
    if data.get("source_type") not in ("http", "local"):
        return "source_type must be 'http' or 'local'"
    if not data.get("file_path"):
        return "file_path is required"
    if not data.get("csv_timestamp_column"):
        return "csv_timestamp_column is required"
    if not data.get("csv_timestamp_format"):
        return "csv_timestamp_format is required"
    if schedule := data.get("schedule"):
        if schedule.get("period") not in ("days", "hours", "minutes"):
            return "schedule.period must be 'days', 'hours', or 'minutes'"
        if not isinstance(schedule.get("interval"), (int, float)) or schedule["interval"] <= 0:
            return "schedule.interval must be a positive number"
    return None
