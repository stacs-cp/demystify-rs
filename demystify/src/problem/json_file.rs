use std::fs::File;
use std::io::{BufReader, Write};
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub const ZSTD_LEVEL: i32 = 9;

fn compressed(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "zst")
}

pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let file = File::open(path).with_context(|| format!("Opening {}", path.display()))?;
    if compressed(path) {
        #[cfg(target_arch = "wasm32")]
        anyhow::bail!("Compressed filesystem input is unavailable on wasm32");
        #[cfg(not(target_arch = "wasm32"))]
        {
            let decoder = zstd::stream::read::Decoder::new(file)
                .with_context(|| format!("Opening zstd stream {}", path.display()))?;
            return serde_json::from_reader(decoder)
                .with_context(|| format!("Reading compressed JSON {}", path.display()));
        }
    }
    serde_json::from_reader(BufReader::new(file))
        .with_context(|| format!("Reading JSON {}", path.display()))
}

pub fn write_atomic<T: Serialize>(path: &Path, value: &T, pretty: bool) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    if compressed(path) {
        #[cfg(target_arch = "wasm32")]
        anyhow::bail!("Compressed filesystem output is unavailable on wasm32");
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut encoder =
                zstd::stream::write::Encoder::new(temporary.as_file_mut(), ZSTD_LEVEL)
                    .with_context(|| format!("Creating zstd stream {}", path.display()))?;
            encoder.include_checksum(true)?;
            if pretty {
                serde_json::to_writer_pretty(&mut encoder, value)?;
            } else {
                serde_json::to_writer(&mut encoder, value)?;
            }
            encoder.write_all(b"\n")?;
            encoder.finish()?;
        }
    } else {
        if pretty {
            serde_json::to_writer_pretty(&mut temporary, value)?;
        } else {
            serde_json::to_writer(&mut temporary, value)?;
        }
        temporary.write_all(b"\n")?;
    }
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("Replacing {}", path.display()))?;
    Ok(())
}
