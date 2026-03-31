import uuid
import logging
from datetime import datetime, time
from apscheduler.schedulers.background import BackgroundScheduler
from apscheduler.triggers.interval import IntervalTrigger

log = logging.getLogger(__name__)

scheduler = BackgroundScheduler(daemon=True)


def build_trigger(schedule: dict) -> IntervalTrigger:
    """Build an IntervalTrigger from a schedule dict."""
    period = schedule["period"]        # "days" | "hours" | "minutes"
    interval = int(schedule["interval"])
    start_time_str = schedule.get("start_time")  # "HH:MM"

    # Parse start_time into a datetime for the trigger's start_date
    if start_time_str:
        now = datetime.now()
        parsed = time.fromisoformat(start_time_str)
        start_date = now.replace(
            hour=parsed.hour,
            minute=parsed.minute,
            second=0,
            microsecond=0,
        )
        # If the start time has already passed today, begin from the next interval
        if start_date < now:
            from datetime import timedelta
            if period == "days":
                start_date += timedelta(days=interval)
            elif period == "hours":
                start_date += timedelta(hours=interval)
            elif period == "minutes":
                start_date += timedelta(minutes=interval)
    else:
        start_date = None

    kwargs = {"start_date": start_date} if start_date else {}

    if period == "days":
        return IntervalTrigger(days=interval, **kwargs)
    elif period == "hours":
        return IntervalTrigger(hours=interval, **kwargs)
    elif period == "minutes":
        return IntervalTrigger(minutes=interval, **kwargs)
    else:
        raise ValueError(f"Unknown period: {period}")


def execute_task(task_id: str) -> None:
    """Run a task and record the result as a TaskRun."""
    from database import get_session
    from models import Task, TaskRun

    log.info(f"Executing task {task_id}")

    with get_session() as session:
        task = session.get(Task, task_id)
        if not task:
            log.error(f"Task {task_id} not found")
            return

        run = TaskRun(
            id=str(uuid.uuid4()),
            task_id=task_id,
            status="started",
            started_at=datetime.utcnow(),
        )
        session.add(run)
        session.commit()
        run_id = run.id

    try:
        result = _run_etl(task_id)

        with get_session() as session:
            run = session.get(TaskRun, run_id)
            run.status = "success"
            run.completed_at = datetime.utcnow()
            run.success_count = result.get("success_count")
            run.failure_count = result.get("failure_count")
            run.skipped_count = result.get("skipped_count")
            run.values_loaded_total = result.get("values_loaded_total")
            run.earliest_timestamp = result.get("earliest_timestamp")
            run.latest_timestamp = result.get("latest_timestamp")
            session.commit()

    except Exception as e:
        log.exception(f"Task {task_id} failed: {e}")
        with get_session() as session:
            run = session.get(TaskRun, run_id)
            run.status = "failure"
            run.completed_at = datetime.utcnow()
            run.error_message = str(e)
            session.commit()


def _run_etl(task_id: str) -> dict:
    """
    Placeholder for hydroserverpy ETL execution.
    Replace this with real hydroserverpy calls when ready.
    """
    from database import get_session
    from models import Task

    with get_session() as session:
        task = session.get(Task, task_id)
        connection = task.connection

        # TODO: wire up hydroserverpy here, e.g.:
        # from hydroserverpy import ETLPipeline
        # pipeline = ETLPipeline(
        #     source_type=task.source_type,
        #     file_path=task.file_path,
        #     csv_delimiter=task.csv_delimiter,
        #     csv_header_row=task.csv_header_row,
        #     csv_timestamp_column=task.csv_timestamp_column,
        #     csv_timestamp_format=task.csv_timestamp_format,
        #     column_mappings=task.column_mappings,
        #     connection_host=connection.host,
        #     auth_type=connection.auth_type,
        #     api_key=connection.api_key,
        #     username=connection.username,
        #     password=connection.password,
        # )
        # return pipeline.run()

        log.info(f"ETL placeholder for task '{task.name}' — hydroserverpy not yet wired")
        return {
            "success_count": 0,
            "failure_count": 0,
            "skipped_count": 0,
            "values_loaded_total": 0,
            "earliest_timestamp": None,
            "latest_timestamp": None,
        }


def register_task(task: dict) -> None:
    """Add or replace a task's job in the scheduler."""
    if not task.get("schedule") or not task.get("is_active"):
        remove_task(task["id"])
        return

    try:
        trigger = build_trigger(task["schedule"])
    except (KeyError, ValueError) as e:
        log.error(f"Invalid schedule for task {task['id']}: {e}")
        return

    job_id = f"task_{task['id']}"

    if scheduler.get_job(job_id):
        scheduler.reschedule_job(job_id, trigger=trigger)
        log.info(f"Rescheduled job {job_id}")
    else:
        scheduler.add_job(
            execute_task,
            trigger=trigger,
            id=job_id,
            args=[task["id"]],
            replace_existing=True,
        )
        log.info(f"Registered job {job_id}")


def remove_task(task_id: str) -> None:
    """Remove a task's job from the scheduler if it exists."""
    job_id = f"task_{task_id}"
    if scheduler.get_job(job_id):
        scheduler.remove_job(job_id)
        log.info(f"Removed job {job_id}")


def load_all_tasks() -> None:
    """On startup, reload all active scheduled tasks from the DB."""
    from database import get_session
    from models import Task

    with get_session() as session:
        tasks = session.query(Task).filter(Task.is_active == True).all()
        task_dicts = [t.to_dict() for t in tasks]

    for task_dict in task_dicts:
        if task_dict.get("schedule"):
            register_task(task_dict)
    log.info(f"Loaded {len(task_dicts)} active tasks from database")
