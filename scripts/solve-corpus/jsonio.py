import json
import os
from pathlib import Path

import zstandard


LEVEL = 9


def read_bytes(path):
    path = Path(path)
    if path.suffix != '.zst':
        return path.read_bytes()
    with path.open('rb') as compressed:
        with zstandard.ZstdDecompressor().stream_reader(compressed) as reader:
            return reader.read()


def read_json(path):
    return json.loads(read_bytes(path))


def write_text(path, contents):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + '.tmp')
    if path.suffix == '.zst':
        with temporary.open('wb') as output:
            output.write(zstandard.ZstdCompressor(level=LEVEL, write_checksum=True)
                         .compress(contents.encode()))
            output.flush()
            os.fsync(output.fileno())
    else:
        with temporary.open('w') as output:
            output.write(contents)
            output.flush()
            os.fsync(output.fileno())
    temporary.replace(path)


def compress_file(source, destination):
    source = Path(source)
    destination = Path(destination)
    assert destination.suffix == '.zst'
    assert source != destination
    temporary = destination.with_name(destination.name + '.tmp')
    with source.open('rb') as input_stream, temporary.open('wb') as output_stream:
        compressor = zstandard.ZstdCompressor(level=LEVEL, write_checksum=True)
        compressor.copy_stream(input_stream, output_stream)
        output_stream.flush()
        os.fsync(output_stream.fileno())
    temporary.replace(destination)
    source.unlink()
