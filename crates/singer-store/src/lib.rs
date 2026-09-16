use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub struct Wal<E> {
    path: PathBuf,
    file: File,
    _event: PhantomData<E>,
}

impl<E: Serialize> Wal<E> {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            path,
            file,
            _event: PhantomData,
        })
    }

    pub fn append(&mut self, event: &E) -> Result<(), StoreError> {
        serde_json::to_writer(&mut self.file, event)?;
        self.file.write_all(b"\n")?;
        Ok(())
    }

    pub fn commit(&mut self) -> Result<(), StoreError> {
        self.file.flush()?;
        self.file.sync_data()?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl<E: DeserializeOwned> Wal<E> {
    pub fn replay(path: impl AsRef<Path>) -> Result<Vec<E>, StoreError> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err.into()),
        };

        let mut out = Vec::new();
        for line in BufReader::new(file).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            out.push(serde_json::from_str(&line)?);
        }
        Ok(out)
    }
}

pub fn write_snapshot<T: Serialize>(path: impl AsRef<Path>, value: &T) -> Result<(), StoreError> {
    let path = path.as_ref();
    let tmp = path.with_extension("tmp");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&tmp)?;
    serde_json::to_writer(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp, path)?;
    if let Some(parent) = path.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

pub fn read_snapshot<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<Option<T>, StoreError> {
    match File::open(path) {
        Ok(file) => Ok(Some(serde_json::from_reader(BufReader::new(file))?)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}
