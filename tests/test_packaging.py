"""Negative checks for release integrity: corruption must stop publication."""
import importlib.util
import io
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("verify_packages", Path(__file__).resolve().parents[1] / "scripts/verify-packages.py")
verify = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verify)


class ReleaseIntegrity(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.files = verify.inventory("0.1.0")
        for name in self.files.values():
            (self.root / name).write_bytes(b"release fixture")
        self.manifest = self.root / "SHA256SUMS"
        self.manifest.write_text("".join(f"{verify.digest(b'release fixture')}  {name}\n" for name in self.files.values()))

    def test_complete_release(self):
        verify.verify_inventory(self.root, self.files)
        verify.verify_checksums(self.root, self.files)

    def test_corrupted_download(self):
        (self.root / self.files["deb"]).write_bytes(b"corrupt")
        with self.assertRaisesRegex(ValueError, "Checksum mismatch"):
            verify.verify_checksums(self.root, self.files)

    def test_missing_checksum(self):
        self.manifest.write_text("\n".join(self.manifest.read_text().splitlines()[:-1]) + "\n")
        with self.assertRaisesRegex(ValueError, "every artifact"):
            verify.verify_checksums(self.root, self.files)

    def test_duplicate_checksum(self):
        self.manifest.write_text(self.manifest.read_text() * 2)
        with self.assertRaisesRegex(ValueError, "duplicate"):
            verify.verify_checksums(self.root, self.files)

    def test_checksum_cannot_escape_directory(self):
        self.manifest.write_text(f"{verify.digest(b'release fixture')}  ../outside\n")
        with self.assertRaisesRegex(ValueError, "Invalid checksum"):
            verify.verify_checksums(self.root, self.files)

    def test_wrong_version_or_extra_artifact(self):
        (self.root / "open-meteo_0.0.9_amd64.deb").write_bytes(b"stale")
        with self.assertRaisesRegex(ValueError, "inventory"):
            verify.verify_inventory(self.root, self.files)

    def test_missing_or_linked_artifact(self):
        path = self.root / self.files["deb"]
        path.unlink()
        with self.assertRaisesRegex(ValueError, "inventory"):
            verify.verify_inventory(self.root, self.files)
        path.symlink_to(self.root / self.files["rpm"])
        with self.assertRaisesRegex(ValueError, "regular"):
            verify.verify_inventory(self.root, self.files)

    def test_unsafe_archive_members(self):
        for name, kind in [("../escape", tarfile.REGTYPE), ("/absolute", tarfile.REGTYPE), ("link", tarfile.SYMTYPE), ("device", tarfile.CHRTYPE)]:
            with self.subTest(name=name):
                data = io.BytesIO()
                with tarfile.open(fileobj=data, mode="w") as archive:
                    member = tarfile.TarInfo(name)
                    member.type = kind
                    archive.addfile(member)
                with self.assertRaises(ValueError):
                    verify.tar_contents(data.getvalue())

    def test_missing_launcher_payload(self):
        with self.assertRaisesRegex(ValueError, "Missing test payload"):
            verify.verify_payload({}, "test")

    def test_elf_metadata_changes_preserve_code_signature(self):
        original = Path('/bin/true').read_bytes()
        binary = self.root / 'binary'
        binary.write_bytes(original)
        note = self.root / 'note'
        note.write_bytes(b'packaging metadata')
        subprocess.run(['objcopy', '--add-section', f'.ci-test={note}', str(binary)], check=True)
        self.assertNotEqual(verify.digest(original), verify.digest(binary.read_bytes()))
        self.assertEqual(verify.executable_sections(original), verify.executable_sections(binary.read_bytes()))

    def test_changed_executable_code_is_detected(self):
        original = Path('/bin/true').read_bytes()
        binary = self.root / 'binary'
        binary.write_bytes(original)
        section = self.root / 'text'
        subprocess.run(['objcopy', '--dump-section', f'.text={section}', str(binary)], check=True)
        data = bytearray(section.read_bytes())
        data[-1] ^= 1
        section.write_bytes(data)
        subprocess.run(['objcopy', '--update-section', f'.text={section}', str(binary)], check=True)
        self.assertNotEqual(verify.executable_sections(original), verify.executable_sections(binary.read_bytes()))


if __name__ == "__main__":
    unittest.main()
