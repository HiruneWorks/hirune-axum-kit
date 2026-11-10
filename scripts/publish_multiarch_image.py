"""Publish two tested Linux images and then their shared architecture index."""

import argparse
import json
import re
import subprocess
import sys


def command(*arguments, capture=False):
    return subprocess.run(arguments, check=True, text=True,
                          stdout=subprocess.PIPE if capture else sys.stderr).stdout


def descriptor(image):
    return json.loads(command("docker", "buildx", "imagetools", "inspect", image,
                              "--format", "{{json .Manifest}}", capture=True))


def publish(repository, version, amd64_image, arm64_image):
    if not re.fullmatch(r"ghcr\.io/[a-z0-9][a-z0-9-]*/[a-z0-9][a-z0-9._/-]*", repository):
        raise ValueError("Use a GHCR repository without a tag or digest")
    if not re.fullmatch(r"[0-9][A-Za-z0-9_.-]{0,127}", version):
        raise ValueError("Use a valid version tag")
    images = {"amd64": amd64_image, "arm64": arm64_image}
    for architecture, image in images.items():
        info = json.loads(command("docker", "image", "inspect", image, capture=True))[0]
        if info["Os"] != "linux" or info["Architecture"] != architecture:
            raise ValueError(f"The {architecture} input has the wrong image platform")
    references = []
    for architecture, image in images.items():
        target = f"{repository}:{version}-{architecture}"
        command("docker", "tag", image, target)
        command("docker", "push", target)
        references.append(repository + "@" + descriptor(target)["digest"])
    target = repository + ":" + version
    command("docker", "buildx", "imagetools", "create", "--tag", target, *references)
    index = descriptor(target)
    platforms = {item["platform"]["os"] + "/" + item["platform"]["architecture"]
                 for item in index.get("manifests", [])
                 if item.get("platform", {}).get("architecture") not in (None, "unknown")}
    if platforms != {"linux/amd64", "linux/arm64"}:
        raise ValueError("Published index does not contain both required Linux platforms")
    return {"image": target, "reference": repository + "@" + index["digest"],
            "digest": index["digest"], "platforms": sorted(platforms)}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("repository")
    parser.add_argument("version")
    parser.add_argument("amd64_image")
    parser.add_argument("arm64_image")
    arguments = parser.parse_args()
    print(publish(arguments.repository, arguments.version, arguments.amd64_image, arguments.arm64_image)["reference"])
