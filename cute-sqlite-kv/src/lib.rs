/// This crate provides a simple multi-process key-value store,
///  using SQLite as the underlying storage.
///
/// This code is intended to be as simple as a wrapper around
/// sqlite (via rusqlite) as possible, while ensuring the same
/// KVStore can be used from multiple processes correctly.
///
/// # Examples
///
/// ```
/// use cute_sqlite_kv::KVStore;
///
/// // Create a new in-memory store
/// let kvstore = KVStore::new_in_memory().unwrap();
///
/// // Set a key-value pair
/// kvstore.set("key", "value").unwrap();
///
/// // Get the value for a key
/// let result = kvstore.get("key").unwrap();
/// assert_eq!(result, Some("value".to_string()));
///
/// // Delete a key
/// kvstore.delete("key").unwrap();
///
/// // Check if the key is deleted
/// let result = kvstore.get("key").unwrap();
/// assert_eq!(result, None);
/// ```
///
/// # Usage
///
/// To use the `KVStore` struct, you need to import the `cute_sqlite_kv` crate and create a new KVStore instance.
/// You can create a KVStore either in-memory or from a file.
///
/// ## In-Memory KVStore
///
/// To create a new in-memory KVStore, use the `new_in_memory` method:
///
/// ```rust
/// use cute_sqlite_kv::KVStore;
///
/// let kvstore = KVStore::new_in_memory().unwrap();
/// ```
///
/// ## File-based KVStore
///
/// To create a new KVStore using a file as the storage, use the `new_from_file` method and provide the path to the file:
///
/// ```rust
/// use cute_sqlite_kv::KVStore;
/// use std::path::Path;
///
/// let filename = Path::new("/tmp/kvstore.db");
/// let kvstore = KVStore::new_from_file(filename).unwrap();
/// ```
///
/// # Methods
///
/// The `KVStore` struct provides the following methods:
///
/// - `set`: Sets a key-value pair in the `KVStore`.
/// - `get`: Retrieves the value for a given key from the `KVStore`.
/// - `delete`: Deletes a key-value pair from the `KVStore`.
///
/// Please refer to the method documentation for more details on how to use each method.
///
/// # Examples
///
/// ```rust
/// use cute_sqlite_kv::KVStore;
///
/// let kvstore = KVStore::new_in_memory().unwrap();
///
/// kvstore.set("key", "value").unwrap();
///
/// let result = kvstore.get("key").unwrap();
/// assert_eq!(result, Some("value".to_string()));
///
/// kvstore.delete("key").unwrap();
///
/// let result = kvstore.get("key").unwrap();
/// assert_eq!(result, None);
/// ```
///
use std::path::Path;

use rusqlite::Connection;

const KEY_COLUMN: &str = "KVStore_key";
const VAL_COLUMN: &str = "KVStore_val";
const TABLE: &str = "KVStore_table";

pub struct KVStore {
    connection: Connection,
}

impl KVStore {
    /// Creates a new in-memory key-value store.
    ///
    /// An in-memory key-value store is in practice worse than
    /// a standard HashMap in every way, so the only use of this function
    /// is for creating a key value store for testing.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use cute_sqlite_kv::KVStore;
    ///
    /// let kvstore = KVStore::new_in_memory().unwrap();
    /// ```
    pub fn new_in_memory() -> rusqlite::Result<KVStore> {
        let connection = Connection::open_in_memory()?;
        let kvstore = KVStore { connection };
        kvstore.create_table()?;
        Ok(kvstore)
    }

    /// Creates a new KVStore using a file as the storage.
    ///
    /// # Arguments
    ///
    /// * `filename` - The path to the file used as the storage for the KVStore.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use cute_sqlite_kv::KVStore;
    /// use std::path::Path;
    ///
    /// let filename = Path::new("/tmp/kvstore.db");
    /// let kvstore = KVStore::new_from_file(filename).unwrap();
    /// ```
    pub fn new_from_file(filename: &Path) -> rusqlite::Result<KVStore> {
        let connection = Connection::open(filename)?;
        let kvstore = KVStore { connection };
        kvstore.create_table()?;
        Ok(kvstore)
    }

    /// Internal function which ensures KVStore
    /// table is created
    fn create_table(&self) -> rusqlite::Result<()> {
        self.connection.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {} (
                {} varchar PRIMARY KEY UNIQUE NOT NULL,
                {}
            )",
                TABLE, KEY_COLUMN, VAL_COLUMN
            ),
            (),
        )?;
        Ok(())
    }

    /// Sets a key-value pair in the KVStore.
    /// Overwrites any existing value
    ///
    /// # Arguments
    ///
    /// * `key` - The key for the value.
    /// * `value` - The value to be stored.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use cute_sqlite_kv::KVStore;
    ///
    /// let kvstore = KVStore::new_in_memory().unwrap();
    ///
    /// kvstore.set("key", "value").unwrap();
    /// ```
    pub fn set(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.connection.execute(
            &format!(
                "REPLACE INTO {} ({}, {}) VALUES (?, ?)",
                TABLE, KEY_COLUMN, VAL_COLUMN
            ),
            &[key, value],
        )?;
        Ok(())
    }

    /// Retrieves the value for a given key from the KVStore.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to retrieve the value for.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use cute_sqlite_kv::KVStore;
    ///
    /// let kvstore = KVStore::new_in_memory().unwrap();
    ///
    /// kvstore.set("key", "value").unwrap();
    ///
    /// let result = kvstore.get("key").unwrap();
    /// assert_eq!(result, Some("value".to_string()));
    /// ```

    pub fn get(&self, key: &str) -> rusqlite::Result<Option<String>> {
        let mut stmt = self.connection.prepare(&format!(
            "SELECT {} FROM {} WHERE {} = ?",
            VAL_COLUMN, TABLE, KEY_COLUMN
        ))?;
        let mut rows = stmt.query(&[key])?;
        if let Some(row) = rows.next()? {
            let value: String = row.get(0)?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    /// Deletes a key-value pair from the KVStore,
    /// if present
    ///
    /// # Arguments
    ///
    /// * `key` - The key to delete.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use cute_sqlite_kv::KVStore;
    ///
    /// let kvstore = KVStore::new_in_memory().unwrap();
    ///
    /// kvstore.set("key", "value").unwrap();
    ///
    /// kvstore.delete("key").unwrap();
    ///
    /// let result = kvstore.get("key").unwrap();
    /// assert_eq!(result, None);
    /// ```
    pub fn delete(&self, key: &str) -> rusqlite::Result<()> {
        self.connection.execute(
            &format!("DELETE FROM {} WHERE {} = ?", TABLE, KEY_COLUMN),
            &[key],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::tempdir;

    #[test]
    fn test_new_in_memory() {
        let _ = KVStore::new_in_memory().unwrap();
    }

    #[test]
    fn test_new_from_file() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let filename = temp_dir.path().join("kvstore.db");
        let _ = KVStore::new_from_file(&filename).unwrap();
    }

    #[test]
    fn test_new_from_file_more() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let filename = temp_dir.path().join("kvstore.db");
        let kvstore = KVStore::new_from_file(&filename).unwrap();
        let key = "test_key";
        let value = "test_value";
        kvstore.set(key, value).unwrap();
        let result = kvstore.get(key).unwrap();
        assert_eq!(result, Some(value.to_string()));
    }

    #[test]
    fn test_reopen_database() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
        let filename = temp_dir.path().join("kvstore.db");
        {
            let kvstore = KVStore::new_from_file(&filename).unwrap();
            let key = "test_key";
            let value = "test_value";
            kvstore.set(key, value).unwrap();
        }
        {
            let kvstore = KVStore::new_from_file(&filename).unwrap();
            let key = "test_key";
            let result = kvstore.get(key).unwrap();
            assert_eq!(result, Some("test_value".to_string()));
        }
    }

    #[test]
    fn test_set_and_get() {
        let kvstore = KVStore::new_in_memory().unwrap();
        let key = "test_key";
        let value = "test_value";
        kvstore.set(key, value).unwrap();
        let result = kvstore.get(key).unwrap();
        assert_eq!(result, Some(value.to_string()));
    }

    #[test]
    fn test_get_nonexistent_key() {
        let kvstore = KVStore::new_in_memory().unwrap();
        let key = "nonexistent_key";
        let result = kvstore.get(key).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_delete() {
        let kvstore = KVStore::new_in_memory().unwrap();
        let key = "test_key";
        let value = "test_value";
        kvstore.set(key, value).unwrap();
        kvstore.delete(key).unwrap();
        let result = kvstore.get(key).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_delete_nonexistent_key() {
        let kvstore = KVStore::new_in_memory().unwrap();
        let key = "nonexistent_key";
        kvstore.delete(key).unwrap();
        let result = kvstore.get(key).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_many_connections() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
        let filename = temp_dir.path().join("kvstore.db");

        // Create the first connection and add a key
        {
            let kvstore = KVStore::new_from_file(&filename).unwrap();
            let key = "test_key";
            let value = "test_value";
            kvstore.set(key, value).unwrap();
        }

        // Check if the key is there
        {
            let kvstore = KVStore::new_from_file(&filename).unwrap();
            let key = "test_key";
            let result = kvstore.get(key).unwrap();
            assert_eq!(result, Some("test_value".to_string()));
        }

        // Delete the key
        {
            let kvstore = KVStore::new_from_file(&filename).unwrap();
            let key = "test_key";
            kvstore.delete(key).unwrap();
        }

        // Check if the key is gone
        {
            let kvstore = KVStore::new_from_file(&filename).unwrap();
            let key = "test_key";
            let result = kvstore.get(key).unwrap();
            assert_eq!(result, None);
        }
    }

    #[test]
    fn test_overlapping_connections() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
        let filename = temp_dir.path().join("kvstore.db");

        let kvstore = KVStore::new_from_file(&filename).unwrap();

        // Create the first connection and add a key
        {
            let key = "test_key";
            let value = "test_value";
            kvstore.set(key, value).unwrap();
        }

        let kvstore2 = KVStore::new_from_file(&filename).unwrap();

        // Check if the key is there
        {
            let key = "test_key";
            let result = kvstore2.get(key).unwrap();
            assert_eq!(result, Some("test_value".to_string()));
        }

        // Delete the key
        {
            let key = "test_key";
            kvstore2.delete(key).unwrap();
        }

        // Check if the key is gone
        {
            let key = "test_key";
            let result = kvstore.get(key).unwrap();
            assert_eq!(result, None);
        }
    }

    #[test]
    fn test_insert_multiple_times() {
        let kvstore = KVStore::new_in_memory().unwrap();
        let key = "test_key";
        let value1 = "test_value1";
        let value2 = "test_value2";
        let value3 = "test_value3";

        kvstore.set(key, value1).unwrap();
        let result1 = kvstore.get(key).unwrap();
        assert_eq!(result1, Some(value1.to_string()));

        kvstore.set(key, value2).unwrap();
        let result2 = kvstore.get(key).unwrap();
        assert_eq!(result2, Some(value2.to_string()));

        kvstore.set(key, value3).unwrap();
        let result3 = kvstore.get(key).unwrap();
        assert_eq!(result3, Some(value3.to_string()));
    }
}
