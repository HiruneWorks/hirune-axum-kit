import json
import unittest
from unittest.mock import patch

import publish_multiarch_image as publication


class MultiarchPublicationTests(unittest.TestCase):
    def fake_command(self, *arguments, capture=False):
        self.calls.append(arguments)
        if arguments[:3] == ("docker", "image", "inspect"):
            return json.dumps([{"Architecture": arguments[3], "Os": "linux"}])
        if arguments[:4] == ("docker", "buildx", "imagetools", "inspect"):
            target = arguments[4]
            if target.endswith(("-amd64", "-arm64")):
                return json.dumps({"digest": "sha256:" + target.rsplit("-", 1)[1]})
            return json.dumps({"digest": "sha256:index", "manifests": [
                {"platform": {"os": "linux", "architecture": value}} for value in self.platforms]})

    def setUp(self):
        self.calls = []
        self.platforms = ["amd64", "arm64"]

    def test_index_is_published_after_both_exact_inputs_are_pushed(self):
        with patch.object(publication, "command", side_effect=self.fake_command):
            result = publication.publish("ghcr.io/example/service", "1.2.3", "amd64", "arm64")
        pushes = [i for i, args in enumerate(self.calls) if args[:2] == ("docker", "push")]
        create = next(i for i, args in enumerate(self.calls) if args[:4] == ("docker", "buildx", "imagetools", "create"))
        self.assertEqual(len(pushes), 2)
        self.assertTrue(all(i < create for i in pushes))
        self.assertFalse(any("build" in args for args in self.calls))
        self.assertEqual(result["reference"], "ghcr.io/example/service@sha256:index")

    def test_wrong_input_architecture_is_rejected_before_publication(self):
        with patch.object(publication, "command", side_effect=self.fake_command):
            with self.assertRaisesRegex(ValueError, "wrong image platform"):
                publication.publish("ghcr.io/example/service", "1.2.3", "arm64", "amd64")
        self.assertFalse(any(args[:2] == ("docker", "push") for args in self.calls))

    def test_missing_index_platform_is_a_publication_failure(self):
        self.platforms = ["amd64"]
        with patch.object(publication, "command", side_effect=self.fake_command):
            with self.assertRaisesRegex(ValueError, "both required"):
                publication.publish("ghcr.io/example/service", "1.2.3", "amd64", "arm64")


if __name__ == "__main__":
    unittest.main()
