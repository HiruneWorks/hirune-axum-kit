import copy
import unittest

from verify_container_archive import verify_configuration


class ContainerArchiveIdentityTests(unittest.TestCase):
    def setUp(self):
        self.expected = {"architecture": "arm64", "os": "linux", "rootfs": {"diff_ids": ["sha256:layer"]}, "config": {"User": "10001", "Entrypoint": ["/app/api"], "Env": ["MODE=development"]}}
        self.actual = {"Id": "sha256:manifest", "Architecture": "arm64", "Os": "linux", "RootFS": {"Layers": ["sha256:layer"]}, "Config": copy.deepcopy(self.expected["config"])}

    def test_store_manifest_id_does_not_replace_content_verification(self):
        verify_configuration(self.expected, self.actual)
        self.actual["Id"] = "sha256:config"
        verify_configuration(self.expected, self.actual)

    def test_wrong_platform_is_rejected(self):
        self.actual["Architecture"] = "amd64"
        with self.assertRaisesRegex(ValueError, "Architecture"):
            verify_configuration(self.expected, self.actual)

    def test_changed_filesystem_is_rejected(self):
        self.actual["RootFS"]["Layers"] = ["sha256:other-layer"]
        with self.assertRaisesRegex(ValueError, "filesystem"):
            verify_configuration(self.expected, self.actual)

    def test_changed_execution_configuration_is_rejected(self):
        for field, value in [("User", "0"), ("Entrypoint", ["/other"]), ("Env", ["MODE=other"])]:
            with self.subTest(field=field):
                changed = copy.deepcopy(self.actual)
                changed["Config"][field] = value
                with self.assertRaisesRegex(ValueError, field):
                    verify_configuration(self.expected, changed)


if __name__ == "__main__":
    unittest.main()
