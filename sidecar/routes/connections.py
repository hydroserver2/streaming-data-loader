import uuid
from flask import Blueprint, jsonify, request
from database import get_session
from models import HydroServerConnection

bp = Blueprint("connections", __name__, url_prefix="/connections")


@bp.get("/")
def list_connections():
    with get_session() as session:
        connections = session.query(HydroServerConnection).all()
        return jsonify([c.to_dict() for c in connections])


@bp.get("/<string:connection_id>")
def get_connection(connection_id: str):
    with get_session() as session:
        connection = session.get(HydroServerConnection, connection_id)
        if not connection:
            return jsonify({"error": "Connection not found"}), 404
        return jsonify(connection.to_dict())


@bp.post("/")
def create_connection():
    data = request.get_json()
    if not data:
        return jsonify({"error": "No data provided"}), 400

    errors = _validate_connection(data)
    if errors:
        return jsonify({"error": errors}), 422

    with get_session() as session:
        connection = HydroServerConnection(
            id=str(uuid.uuid4()),
            name=data["name"],
            host=data["host"],
            auth_type=data["auth_type"],
            api_key=data.get("api_key"),
            username=data.get("username"),
            password=data.get("password"),
        )
        session.add(connection)
        session.commit()
        session.refresh(connection)
        return jsonify(connection.to_dict()), 201


@bp.put("/<string:connection_id>")
def update_connection(connection_id: str):
    data = request.get_json()
    if not data:
        return jsonify({"error": "No data provided"}), 400

    errors = _validate_connection(data)
    if errors:
        return jsonify({"error": errors}), 422

    with get_session() as session:
        connection = session.get(HydroServerConnection, connection_id)
        if not connection:
            return jsonify({"error": "Connection not found"}), 404

        connection.name = data["name"]
        connection.host = data["host"]
        connection.auth_type = data["auth_type"]
        connection.api_key = data.get("api_key")
        connection.username = data.get("username")
        connection.password = data.get("password")

        session.commit()
        session.refresh(connection)
        return jsonify(connection.to_dict())


@bp.delete("/<string:connection_id>")
def delete_connection(connection_id: str):
    with get_session() as session:
        connection = session.get(HydroServerConnection, connection_id)
        if not connection:
            return jsonify({"error": "Connection not found"}), 404
        session.delete(connection)
        session.commit()
        return jsonify({"ok": True})


def _validate_connection(data: dict) -> str | None:
    if not data.get("name"):
        return "name is required"
    if not data.get("host"):
        return "host is required"
    if data.get("auth_type") not in ("apikey", "userpass"):
        return "auth_type must be 'apikey' or 'userpass'"
    if data["auth_type"] == "apikey" and not data.get("api_key"):
        return "api_key is required when auth_type is 'apikey'"
    if data["auth_type"] == "userpass" and not (data.get("username") and data.get("password")):
        return "username and password are required when auth_type is 'userpass'"
    return None
