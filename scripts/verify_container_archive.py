"""Verify loaded runtime configuration and layers against a Docker save archive."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile


def verify_configuration(expected, actual):
    for name, value in [("Architecture", expected["architecture"]), ("Os", expected["os"])]:
        if actual.get(name) != value:
            raise ValueError(f"Loaded image changed {name}")
    if actual.get("RootFS", {}).get("Layers") != expected["rootfs"]["diff_ids"]:
        raise ValueError("Loaded image changed filesystem layers")
    for name, value in expected["config"].items():
        if actual.get("Config", {}).get(name) != value:
            raise ValueError(f"Loaded image changed runtime configuration: {name}")


def verify_archive(archive_path, image):
    with tarfile.open(archive_path) as archive:
        entries = json.load(archive.extractfile("manifest.json"))
        entries = [entry for entry in entries if image in entry.get("RepoTags", [])]
        if len(entries) != 1:
            raise ValueError("Archive must contain exactly one matching image tag")
        config_name = entries[0]["Config"]
        raw = archive.extractfile(config_name).read()
        digest = hashlib.sha256(raw).hexdigest()
        if Path(config_name).name.removesuffix(".json") != digest:
            raise ValueError("Image configuration digest does not match its content")
        expected = json.loads(raw)
    actual = json.loads(subprocess.check_output(["docker", "image", "inspect", image], text=True))[0]
    verify_configuration(expected, actual)
    print(f"Verified exact image config sha256:{digest} and {len(expected['rootfs']['diff_ids'])} filesystem layers")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("image")
    arguments = parser.parse_args()
    verify_archive(arguments.archive, arguments.image)
