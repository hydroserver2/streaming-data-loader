from flask import Blueprint, jsonify, request
from database import get_session
from models import TaskRun

bp = Blueprint("runs", __name__, url_prefix="/runs")


@bp.get("/")
def list_runs():
    """List all runs, optionally filtered by task_id."""
    task_id = request.args.get("task_id")
    with get_session() as session:
        query = session.query(TaskRun).order_by(TaskRun.started_at.desc())
        if task_id:
            query = query.filter(TaskRun.task_id == task_id)
        runs = query.all()
        return jsonify([r.to_dict() for r in runs])


@bp.get("/<string:run_id>")
def get_run(run_id: str):
    with get_session() as session:
        run = session.get(TaskRun, run_id)
        if not run:
            return jsonify({"error": "Run not found"}), 404
        return jsonify(run.to_dict())
