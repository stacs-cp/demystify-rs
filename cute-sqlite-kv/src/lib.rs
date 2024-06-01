/// This crate provides a simple key-value cache implementation using SQLite as the underlying storage.
///
/// This code is intended to be as simple as a wrapper around
/// sqlite (via rusqlite) as possible, while ensuring the same
/// cache can be used from multiple processes correctly.
///
/// # Examples
///
/// ```
/// use cute_sqlite_kv::Cache;
///
/// // Create a new in-memory cache
/// let cache = Cache::new_in_memory().unwrap();
///
/// // Set a key-value pair
/// cache.set("key", "value").unwrap();
///
/// // Get the value for a key
/// let result = cache.get("key").unwrap();
/// assert_eq!(result, Some("value".to_string()));
///
/// // Delete a key
/// cache.delete("key").unwrap();
///
/// // Check if the key is deleted
/// let result = cache.get("key").unwrap();
/// assert_eq!(result, None);
/// ```
///
/// # Usage
///
/// To use the `Cache` struct, you need to import the `cute_sqlite_kv` crate and create a new cache instance.
/// You can create a cache either in-memory or from a file.
///
/// ## In-Memory Cache
///
/// To create a new in-memory cache, use the `new_in_memory` method:
///
/// ```rust
/// use cute_sqlite_kv::Cache;
///
/// let cache = Cache::new_in_memory().unwrap();
/// ```
///
/// ## File-based Cache
///
/// To create a new cache using a file as the storage, use the `new_from_file` method and provide the path to the file:
///
/// ```rust
/// use cute_sqlite_kv::Cache;
/// use std::path::Path;
///
/// let filename = Path::new("/tmp/cache.db");
/// let cache = Cache::new_from_file(filename).unwrap();
/// ```
///
/// # Methods
///
/// The `Cache` struct provides the following methods:
///
/// - `set`: Sets a key-value pair in the cache.
/// - `get`: Retrieves the value for a given key from the cache.
/// - `delete`: Deletes a key-value pair from the cache.
///
/// Please refer to the method documentation for more details on how to use each method.
///
/// # Examples
///
/// ```rust
/// use cute_sqlite_kv::Cache;
///
/// let cache = Cache::new_in_memory().unwrap();
///
/// cache.set("key", "value").unwrap();
///
/// let result = cache.get("key").unwrap();
/// assert_eq!(result, Some("value".to_string()));
///
/// cache.delete("key").unwrap();
///
/// let result = cache.get("key").unwrap();
/// assert_eq!(result, None);
/// ```
///
use std::path::Path;

use rusqlite::Connection;

const KEY_COLUMN: &str = "cache_key";
const VAL_COLUMN: &str = "cache_val";
const TABLE: &str = "cache_table";

pub struct Cache {
    connection: Connection,
}

impl Cache {
    /// Creates a new in-memory cache.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use cute_sqlite_kv::Cache;
    ///
    /// let cache = Cache::new_in_memory().unwrap();
    /// ```
    pub fn new_in_memory() -> rusqlite::Result<Cache> {
        let connection = Connection::open_in_memory()?;
        let cache = Cache { connection };
        cache.create_table()?;
        Ok(cache)
    }

    /// Creates a new cache using a file as the storage.
    ///
    /// # Arguments
    ///
    /// * `filename` - The path to the file used as the storage for the cache.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use cute_sqlite_kv::Cache;
    /// use std::path::Path;
    ///
    /// let filename = Path::new("/tmp/cache.db");
    /// let cache = Cache::new_from_file(filename).unwrap();
    /// ```
    pub fn new_from_file(filename: &Path) -> rusqlite::Result<Cache> {
        let connection = Connection::open(filename)?;
        let cache = Cache { connection };
        cache.create_table()?;
        Ok(cache)
    }

    /// Internal function which ensures cache
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

    /// Sets a key-value pair in the cache.
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
    /// use cute_sqlite_kv::Cache;
    ///
    /// let cache = Cache::new_in_memory().unwrap();
    ///
    /// cache.set("key", "value").unwrap();
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

    /// Retrieves the value for a given key from the cache.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to retrieve the value for.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use cute_sqlite_kv::Cache;
    ///
    /// let cache = Cache::new_in_memory().unwrap();
    ///
    /// cache.set("key", "value").unwrap();
    ///
    /// let result = cache.get("key").unwrap();
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

    /// Deletes a key-value pair from the cache,
    /// if present
    ///
    /// # Arguments
    ///
    /// * `key` - The key to delete.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use cute_sqlite_kv::Cache;
    ///
    /// let cache = Cache::new_in_memory().unwrap();
    ///
    /// cache.set("key", "value").unwrap();
    ///
    /// cache.delete("key").unwrap();
    ///
    /// let result = cache.get("key").unwrap();
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
        let _ = Cache::new_in_memory().unwrap();
    }

    #[test]
    fn test_new_from_file() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let filename = temp_dir.path().join("cache.db");
        let _ = Cache::new_from_file(&filename).unwrap();
    }

    #[test]
    fn test_new_from_file_more() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let filename = temp_dir.path().join("cache.db");
        let cache = Cache::new_from_file(&filename).unwrap();
        let key = "test_key";
        let value = "test_value";
        cache.set(key, value).unwrap();
        let result = cache.get(key).unwrap();
        assert_eq!(result, Some(value.to_string()));
    }

    #[test]
    fn test_reopen_database() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
        let filename = temp_dir.path().join("cache.db");
        {
            let cache = Cache::new_from_file(&filename).unwrap();
            let key = "test_key";
            let value = "test_value";
            cache.set(key, value).unwrap();
        }
        {
            let cache = Cache::new_from_file(&filename).unwrap();
            let key = "test_key";
            let result = cache.get(key).unwrap();
            assert_eq!(result, Some("test_value".to_string()));
        }
    }

    #[test]
    fn test_set_and_get() {
        let cache = Cache::new_in_memory().unwrap();
        let key = "test_key";
        let value = "test_value";
        cache.set(key, value).unwrap();
        let result = cache.get(key).unwrap();
        assert_eq!(result, Some(value.to_string()));
    }

    #[test]
    fn test_get_nonexistent_key() {
        let cache = Cache::new_in_memory().unwrap();
        let key = "nonexistent_key";
        let result = cache.get(key).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_delete() {
        let cache = Cache::new_in_memory().unwrap();
        let key = "test_key";
        let value = "test_value";
        cache.set(key, value).unwrap();
        cache.delete(key).unwrap();
        let result = cache.get(key).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_delete_nonexistent_key() {
        let cache = Cache::new_in_memory().unwrap();
        let key = "nonexistent_key";
        cache.delete(key).unwrap();
        let result = cache.get(key).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_many_connections() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
        let filename = temp_dir.path().join("cache.db");

        // Create the first connection and add a key
        {
            let cache = Cache::new_from_file(&filename).unwrap();
            let key = "test_key";
            let value = "test_value";
            cache.set(key, value).unwrap();
        }

        // Check if the key is there
        {
            let cache = Cache::new_from_file(&filename).unwrap();
            let key = "test_key";
            let result = cache.get(key).unwrap();
            assert_eq!(result, Some("test_value".to_string()));
        }

        // Delete the key
        {
            let cache = Cache::new_from_file(&filename).unwrap();
            let key = "test_key";
            cache.delete(key).unwrap();
        }

        // Check if the key is gone
        {
            let cache = Cache::new_from_file(&filename).unwrap();
            let key = "test_key";
            let result = cache.get(key).unwrap();
            assert_eq!(result, None);
        }
    }

    #[test]
    fn test_overlapping_connections() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
        let filename = temp_dir.path().join("cache.db");

        let cache = Cache::new_from_file(&filename).unwrap();

        // Create the first connection and add a key
        {
            let key = "test_key";
            let value = "test_value";
            cache.set(key, value).unwrap();
        }

        let cache2 = Cache::new_from_file(&filename).unwrap();

        // Check if the key is there
        {
            let key = "test_key";
            let result = cache2.get(key).unwrap();
            assert_eq!(result, Some("test_value".to_string()));
        }

        // Delete the key
        {
            let key = "test_key";
            cache2.delete(key).unwrap();
        }

        // Check if the key is gone
        {
            let key = "test_key";
            let result = cache.get(key).unwrap();
            assert_eq!(result, None);
        }
    }

    #[test]
    fn test_insert_multiple_times() {
        let cache = Cache::new_in_memory().unwrap();
        let key = "test_key";
        let value1 = "test_value1";
        let value2 = "test_value2";
        let value3 = "test_value3";

        cache.set(key, value1).unwrap();
        let result1 = cache.get(key).unwrap();
        assert_eq!(result1, Some(value1.to_string()));

        cache.set(key, value2).unwrap();
        let result2 = cache.get(key).unwrap();
        assert_eq!(result2, Some(value2.to_string()));

        cache.set(key, value3).unwrap();
        let result3 = cache.get(key).unwrap();
        assert_eq!(result3, Some(value3.to_string()));
    }
}
