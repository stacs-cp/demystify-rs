import json
from pathlib import Path
import tempfile
import unittest

from jsonio import compress_file, read_bytes, read_json, write_text


class JsonIoTests(unittest.TestCase):
    def test_plain_and_compressed_roundtrip(self):
        value = {'name': 'compressed', 'values': list(range(1000))}
        contents = json.dumps(value) + '\n'
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ('value.json', 'value.json.zst'):
                path = root / name
                write_text(path, contents)
                self.assertEqual(read_bytes(path), contents.encode())
                self.assertEqual(read_json(path), value)

    def test_compress_file_replaces_plain_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'value.json'
            destination = root / 'value.json.zst'
            source.write_text('{"answer":42}\n')
            compress_file(source, destination)
            self.assertFalse(source.exists())
            self.assertEqual(read_json(destination), {'answer': 42})


if __name__ == '__main__':
    unittest.main()
