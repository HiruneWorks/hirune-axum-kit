"""Run a container image against an isolated PostgreSQL database on the CI host."""

import argparse
import base64
import json
import os
from pathlib import Path
import secrets
import shlex
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import uuid


def command(*arguments, capture=False, environment=None):
    return subprocess.run(arguments, check=True, text=True, env=environment,
                          stdout=subprocess.PIPE if capture else None).stdout


def verify(image):
    name = "arm64-image-" + uuid.uuid4().hex[:12]
    containers = []
    command("docker", "network", "create", name)
    try:
        with tempfile.TemporaryDirectory(prefix="arm64-image-") as directory:
            values = {}
            example = Path(".envs/.example/local/app.env")
            for line in example.read_text(encoding="utf8").splitlines():
                if line.strip() and not line.lstrip().startswith("#"):
                    key, value = line.split("=", 1)
                    values[key] = " ".join(shlex.split(value)) if value else ""
            password = secrets.token_urlsafe(32)
            database = name + "-postgres"
            url = f"postgres://postgres:{password}@{database}:5432/arm64"
            values.update({"DATABASE_URL": url, "MIGRATION_DATABASE_URL": url,
                           "BIND_ADDRESS": "0.0.0.0:3000", "ACTIVE_SECRET_KEY": "arm64-ci",
                           "SECRET_KEYS": json.dumps({"arm64-ci": base64.b64encode(secrets.token_bytes(32)).decode()}),
                           "RATE_LIMIT_KEY": secrets.token_urlsafe(32), "RUST_LOG": "warn"})
            settings = Path(directory) / "app.env"
            settings.write_text("".join(f"{key}={value}\n" for key, value in values.items()))
            settings.chmod(0o600)
            command("docker", "run", "--detach", "--name", database, "--network", name,
                    "--env", "POSTGRES_PASSWORD", "--env", "POSTGRES_DB=arm64", "postgres:18.6-alpine",
                    environment={**os.environ, "POSTGRES_PASSWORD": password})
            containers.append(database)
            for _ in range(60):
                ready = subprocess.run(["docker", "exec", database, "pg_isready", "-h", "127.0.0.1", "-U", "postgres", "-d", "arm64"],
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                if ready.returncode == 0:
                    break
                time.sleep(1)
            else:
                raise RuntimeError("PostgreSQL did not become ready")
            command("docker", "run", "--rm", "--network", name, "--env-file", str(settings),
                    "--entrypoint", "migration", image)
            for executable in ["api", "worker", "maintenance"]:
                container = name + "-" + executable
                arguments = ["docker", "run", "--detach", "--name", container, "--network", name,
                             "--read-only", "--tmpfs", "/tmp:rw,nosuid,size=32m,mode=1777", "--cap-drop", "ALL",
                             "--security-opt", "no-new-privileges:true", "--env-file", str(settings),
                             "--entrypoint", executable]
                if executable == "api":
                    arguments.extend(["-p", "127.0.0.1::3000"])
                command(*arguments, image)
                containers.append(container)
            address = command("docker", "port", name + "-api", "3000/tcp", capture=True).strip()
            base = "http://" + address
            for _ in range(60):
                try:
                    with urllib.request.urlopen(base + "/health/ready", timeout=2) as response:
                        assert response.status == 204
                    break
                except (OSError, urllib.error.URLError):
                    time.sleep(1)
            else:
                raise RuntimeError("Native application did not become ready")
            try:
                urllib.request.urlopen(base + "/api/v1/account", timeout=5)
            except urllib.error.HTTPError as response:
                assert response.code == 401
                assert json.load(response)["error"]["code"] == "unauthenticated"
            else:
                raise AssertionError("Unauthenticated account read unexpectedly succeeded")
            for container in containers:
                assert command("docker", "inspect", "--format", "{{.State.Running}}", container, capture=True).strip() == "true", container
            print(json.dumps({"native_database_migration": True, "api_ready": 204, "account_route_unauthenticated": 401,
                              "worker_running": True, "maintenance_running": True}))
    finally:
        for container in reversed(containers):
            subprocess.run(["docker", "rm", "--force", "--volumes", container], check=False,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        subprocess.run(["docker", "network", "rm", name], check=False,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image")
    verify(parser.parse_args().image)
