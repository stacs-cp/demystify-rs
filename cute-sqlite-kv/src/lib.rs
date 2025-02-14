/// This crate provides a very small and simple multi-process
/// persistant key-value store, using `SQLite` for storage.
///
/// The code is intended to be as simple a wrapper around `SQLite`
/// (via rusqlite) as possible.
///
/// The key-value store created can be used from multiple processes,
/// and also opened multiple times from the same process.
///
/// While `SQLite` can be very quick, this key-value store is not
/// intended for high-performance situations, but when you need
/// something as simple as possible, but still correct. Please feel
/// free to take, extend, and modify this code for your own requirements!
///
/// # Examples
///
/// ```
/// use cute_sqlite_kv::KVStore;
/// use std::path::Path;
///
/// // Create a new key-value store
///
/// let filename = Path::new("/tmp/kvstore.db");
/// let kvstore = KVStore::new_from_file(filename).unwrap();
///
/// // Insert a key-value pair
/// kvstore.insert("key", "value").unwrap();
///
/// // Get the value for a key
/// let result = kvstore.get("key").unwrap();
/// assert_eq!(result, Some("value".to_string()));
///
/// // Remove a key
/// kvstore.remove("key").unwrap();
///
/// // Check if the key is removed
/// let result = kvstore.get("key").unwrap();
/// assert_eq!(result, None);
/// ```
///
/// # Usage
///
/// To use the `KVStore` struct, you need to import the `cute_sqlite_kv` crate and create a new `KVStore` instance.
/// You can create a `KVStore` either in-memory or from a file.
///
/// ## In-Memory `KVStore`
///
/// To create a new in-memory `KVStore`, use the `new_in_memory` method:
///
/// ```rust
/// use cute_sqlite_kv::KVStore;
///
/// let kvstore = KVStore::new_in_memory().unwrap();
/// ```
///
/// ## File-based `KVStore`
///
/// To create a new `KVStore` using a file as the storage, use the `new_from_file` method and provide the path to the file:
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
/// - `insert`: Inserts a key-value pair in the `KVStore`.
/// - `get`: Retrieves the value for a given key from the `KVStore`.
/// - `remove`: removes a key-value pair from the `KVStore`.
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
/// kvstore.insert("key", "value").unwrap();
///
/// let result = kvstore.get("key").unwrap();
/// assert_eq!(result, Some("value".to_string()));
///
/// kvstore.remove("key").unwrap();
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
    /// a standard `HashMap` in every way, so the only use of this function
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

    /// Creates a new `KVStore` using a file as the storage.
    ///
    /// # Arguments
    ///
    /// * `filename` - The path to the file used as the storage for the `KVStore`.
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

    /// Internal function which ensures `KVStore`
    /// table is created
    fn create_table(&self) -> rusqlite::Result<()> {
        self.connection.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {TABLE} (
                {KEY_COLUMN} varchar PRIMARY KEY UNIQUE NOT NULL,
                {VAL_COLUMN}
            )"
            ),
            (),
        )?;
        Ok(())
    }

    /// Inserts a key-value pair in the `KVStore`.
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
    /// kvstore.insert("key", "value").unwrap();
    /// ```
    pub fn insert(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.connection.execute(
            &format!("REPLACE INTO {TABLE} ({KEY_COLUMN}, {VAL_COLUMN}) VALUES (?, ?)"),
            [key, value],
        )?;
        Ok(())
    }

    /// Retrieves the value for a given key from the `KVStore`.
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
    /// kvstore.insert("key", "value").unwrap();
    ///
    /// let result = kvstore.get("key").unwrap();
    /// assert_eq!(result, Some("value".to_string()));
    /// ```
    pub fn get(&self, key: &str) -> rusqlite::Result<Option<String>> {
        let mut stmt = self.connection.prepare(&format!(
            "SELECT {VAL_COLUMN} FROM {TABLE} WHERE {KEY_COLUMN} = ?"
        ))?;
        let mut rows = stmt.query([key])?;
        if let Some(row) = rows.next()? {
            let value: String = row.get(0)?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    /// Removes a key-value pair from the `KVStore`,
    /// if present
    ///
    /// # Arguments
    ///
    /// * `key` - The key to remove.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use cute_sqlite_kv::KVStore;
    ///
    /// let kvstore = KVStore::new_in_memory().unwrap();
    ///
    /// kvstore.insert("key", "value").unwrap();
    ///
    /// kvstore.remove("key").unwrap();
    ///
    /// let result = kvstore.get("key").unwrap();
    /// assert_eq!(result, None);
    /// ```
    pub fn remove(&self, key: &str) -> rusqlite::Result<()> {
        self.connection.execute(
            &format!("DELETE FROM {TABLE} WHERE {KEY_COLUMN} = ?"),
            [key],
        )?;
        Ok(())
    }

    /// Clears the entire table in the `KVStore`.
    ///
    /// This method removes all key-value pairs from the table, effectively clearing the entire store.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use cute_sqlite_kv::KVStore;
    ///
    /// let kvstore = KVStore::new_in_memory().unwrap();
    ///
    /// kvstore.insert("key1", "value1").unwrap();
    /// kvstore.insert("key2", "value2").unwrap();
    ///
    /// kvstore.clear().unwrap();
    ///
    /// let result1 = kvstore.get("key1").unwrap();
    /// let result2 = kvstore.get("key2").unwrap();
    ///
    /// assert_eq!(result1, None);
    /// assert_eq!(result2, None);
    /// ```
    pub fn clear(&self) -> rusqlite::Result<()> {
        self.connection
            .execute(&format!("DELETE FROM {TABLE}"), ())?;
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
        kvstore.insert(key, value).unwrap();
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
            kvstore.insert(key, value).unwrap();
        }
        {
            let kvstore = KVStore::new_from_file(&filename).unwrap();
            let key = "test_key";
            let result = kvstore.get(key).unwrap();
            assert_eq!(result, Some("test_value".to_string()));
        }
    }

    #[test]
    fn test_insert_and_get() {
        let kvstore = KVStore::new_in_memory().unwrap();
        let key = "test_key";
        let value = "test_value";
        kvstore.insert(key, value).unwrap();
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
    fn test_remove() {
        let kvstore = KVStore::new_in_memory().unwrap();
        let key = "test_key";
        let value = "test_value";
        kvstore.insert(key, value).unwrap();
        kvstore.remove(key).unwrap();
        let result = kvstore.get(key).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_remove_nonexistent_key() {
        let kvstore = KVStore::new_in_memory().unwrap();
        let key = "nonexistent_key";
        kvstore.remove(key).unwrap();
        let result = kvstore.get(key).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_clear() {
        let kvstore = KVStore::new_in_memory().unwrap();
        let key = "test_key";
        let value = "test_value";
        kvstore.insert(key, value).unwrap();
        kvstore.clear().unwrap();
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
            kvstore.insert(key, value).unwrap();
        }

        // Check if the key is there
        {
            let kvstore = KVStore::new_from_file(&filename).unwrap();
            let key = "test_key";
            let result = kvstore.get(key).unwrap();
            assert_eq!(result, Some("test_value".to_string()));
        }

        // remove the key
        {
            let kvstore = KVStore::new_from_file(&filename).unwrap();
            let key = "test_key";
            kvstore.remove(key).unwrap();
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
            kvstore.insert(key, value).unwrap();
        }

        let kvstore2 = KVStore::new_from_file(&filename).unwrap();

        // Check if the key is there
        {
            let key = "test_key";
            let result = kvstore2.get(key).unwrap();
            assert_eq!(result, Some("test_value".to_string()));
        }

        // remove the key
        {
            let key = "test_key";
            kvstore2.remove(key).unwrap();
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

        kvstore.insert(key, value1).unwrap();
        let result1 = kvstore.get(key).unwrap();
        assert_eq!(result1, Some(value1.to_string()));

        kvstore.insert(key, value2).unwrap();
        let result2 = kvstore.get(key).unwrap();
        assert_eq!(result2, Some(value2.to_string()));

        kvstore.insert(key, value3).unwrap();
        let result3 = kvstore.get(key).unwrap();
        assert_eq!(result3, Some(value3.to_string()));
    }
}
