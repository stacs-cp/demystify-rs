//! Serialisation helpers shared by every JS-facing entry point.
//!
//! `serde_wasm_bindgen::to_value` is **not** safe to use bare: it encodes
//! Rust maps (`BTreeMap`, `HashMap`, `serde_json::Map`) as JavaScript
//! `Map` objects, not plain `{key: value}` objects.  Consumers that probe
//! the result via `Object.keys`, `JSON.stringify`, or property access then
//! silently see an empty object — exactly the bug that made
//! [`crate::WasmPlanner::difficulties`] look broken in the browser when
//! the search itself was fine.
//!
//! Every export here goes through a [`Serializer`] configured with
//! `serialize_maps_as_objects(true)`, which gives the plain-object shape
//! every docstring on the JS side already promises.  Use [`to_js`] in
//! place of `serde_wasm_bindgen::to_value`.
//!
//! Inputs are unaffected — `serde_wasm_bindgen::from_value` already
//! accepts both shapes.

use serde::Serialize;
use serde_wasm_bindgen::Serializer;
use wasm_bindgen::JsValue;

/// Encode `value` as a `JsValue` with Rust maps serialised as plain JS
/// objects (not `Map`s).  Drop-in for `serde_wasm_bindgen::to_value`.
pub fn to_js<T: Serialize + ?Sized>(value: &T) -> Result<JsValue, serde_wasm_bindgen::Error> {
    value.serialize(&Serializer::new().serialize_maps_as_objects(true))
}
