import alembic.config
from flask import Flask, jsonify
from flask_cors import CORS
from database import DB_DIR
from scheduler import scheduler, load_all_tasks
from routes.connections import bp as connections_bp
from routes.tasks import bp as tasks_bp
from routes.runs import bp as runs_bp

app = Flask(__name__)
CORS(app, origins=["http://localhost:1420", "tauri://localhost"])

app.register_blueprint(connections_bp)
app.register_blueprint(tasks_bp)
app.register_blueprint(runs_bp)


def run_migrations() -> None:
    DB_DIR.mkdir(parents=True, exist_ok=True)
    alembic_args = ["--raiseerr", "upgrade", "head"]
    alembic.config.main(argv=alembic_args)


@app.get("/health")
def health():
    return jsonify({"status": "ok"})


if __name__ == "__main__":
    run_migrations()
    scheduler.start()
    load_all_tasks()
    app.run(host="127.0.0.1", port=5321, debug=False, use_reloader=False)
