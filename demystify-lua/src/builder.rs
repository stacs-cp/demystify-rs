//! Builder wrapper for Lua.
//!
//! Wraps `demystify_builder::PuzzleBuilder` so Lua scripts can construct
//! puzzles in-process without needing Conjure on the host.  See
//! `demystify-builder` for the constraint vocabulary; the Lua surface
//! mirrors it directly.
//!
//! # Quick start
//!
//! ```lua
//! local d = require("demystify")
//! local b = d.Builder.new()
//! b:kind("toy")
//! local g = b:var_bool_matrix("g", {{1, 2}, {1, 2}})
//! local rule = b:con_bool("rule")
//! b:sum_ge(rule,
//!   { g:get({1, 1}):pos(), g:get({1, 2}):pos(),
//!     g:get({2, 1}):pos(), g:get({2, 2}):pos() },
//!   2, "rule", "at least two cells are true")
//! local puzzle = b:build()
//! local planner = d.Planner.new(puzzle)
//! ```
//!
//! # Lua surface
//!
//! ## `demystify.Builder`
//!
//! | Method | Description |
//! |--------|-------------|
//! | `Builder.new()` | Construct an empty builder |
//! | `b:kind(s)` | Set the puzzle's `$#KIND` tag |
//! | `b:info(s)` | Push a `$#INFO` string |
//! | `b:var_bool_matrix(name, dims)` | Declare a `$#VAR` matrix, returns a `BoolMatrix` |
//! | `b:con_bool_matrix(name, dims)` | Declare a `$#CON` family, returns a `BoolMatrix` of activation atoms |
//! | `b:con_bool(name)` | Declare a 0-d `$#CON` activation atom |
//! | `b:aux_bool_matrix(name, dims)` | Declare a `$#AUX` matrix |
//! | `b:aux_bool(name)` | Declare a 0-d `$#AUX` atom |
//! | `b:reveal_bool_matrix(name, dims)` | Declare a `$#REVEAL`-target matrix |
//! | `b:reveal(src, target)` | Register `target` as `src`'s reveal target |
//! | `b:show(var, role)` | Register a `$#SHOW` directive — `role` is a lowercase string |
//! | `b:sum_ge(guard, signed, k, family, description)` | Post `guard → sum ≥ k` |
//! | `b:sum_le(guard, signed, k, family, description)` | Post `guard → sum ≤ k` |
//! | `b:sum_eq(guard, signed, k, family, description)` | Post `guard → sum = k` |
//! | `b:sum_eq_unguarded(signed, k)` | Post `sum = k` unconditionally, no `$#CON` entry |
//! | `b:build()` | Finalise into a Puzzle (consumes the builder) |
//!
//! Dimension lists are tables of `{first, last}` pairs (1-indexed inclusive),
//! e.g. `{{1, 9}, {1, 9}}` for a 9×9 grid.  Pass `{}` for a 0-d (scalar)
//! variable.
//!
//! ## `BoolMatrix`
//!
//! | Method | Description |
//! |--------|-------------|
//! | `m:name()` | The matrix's declared name |
//! | `m:get({i, j, ...})` | The `Atom` at the given (1-indexed) indices |
//! | `m:atoms()` | Flat list of every `Atom`, row-major |
//!
//! ## `Atom` / `Signed`
//!
//! `Atom` has `:pos()` (the atom must be true) and `:neg()` (the atom must
//! be false), each returning a `Signed`.  Pass `Signed` values in the lists
//! supplied to `sum_ge`/`sum_le`/`sum_eq`.

use std::ops::RangeInclusive;
use std::sync::{Arc, Mutex};

use mlua::FromLua;
use mlua::prelude::*;

use demystify_builder::{Atom, BoolMatrix, PuzzleBuilder, ShowRole, Signed};

use crate::puzzle::LuaPuzzle;

/// A single boolean atom in a puzzle being built.  Cheap to clone — wraps a
/// raw SAT literal.
#[derive(Copy, Clone)]
pub struct LuaAtom {
    pub(crate) inner: Atom,
}

impl FromLua for LuaAtom {
    fn from_lua(value: LuaValue, _lua: &Lua) -> LuaResult<Self> {
        match value {
            LuaValue::UserData(ud) => ud.borrow::<LuaAtom>().map(|a| *a),
            _ => Err(LuaError::FromLuaConversionError {
                from: value.type_name(),
                to: "Atom".to_string(),
                message: Some("expected an Atom userdata".to_string()),
            }),
        }
    }
}

impl LuaUserData for LuaAtom {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("pos", |_, this, ()| {
            Ok(LuaSigned {
                inner: this.inner.pos(),
            })
        });
        methods.add_method("neg", |_, this, ()| {
            Ok(LuaSigned {
                inner: this.inner.neg(),
            })
        });
    }
}

/// A signed atom for cardinality constraints — produced by `atom:pos()` or
/// `atom:neg()` on the Lua side.
#[derive(Copy, Clone)]
pub struct LuaSigned {
    pub(crate) inner: Signed,
}

impl FromLua for LuaSigned {
    fn from_lua(value: LuaValue, _lua: &Lua) -> LuaResult<Self> {
        match value {
            LuaValue::UserData(ud) => ud.borrow::<LuaSigned>().map(|s| *s),
            _ => Err(LuaError::FromLuaConversionError {
                from: value.type_name(),
                to: "Signed".to_string(),
                message: Some(
                    "expected a Signed userdata (got via atom:pos() / atom:neg())".to_string(),
                ),
            }),
        }
    }
}

impl LuaUserData for LuaSigned {}

/// A matrix of boolean atoms declared via `var_bool_matrix` / `con_bool_matrix`
/// / `aux_bool_matrix`.  Cloning is cheap — the underlying `BoolMatrix`
/// stores `Atom`s, which are themselves just SAT literal handles.
#[derive(Clone)]
pub struct LuaBoolMatrix {
    pub(crate) inner: BoolMatrix,
}

impl FromLua for LuaBoolMatrix {
    fn from_lua(value: LuaValue, _lua: &Lua) -> LuaResult<Self> {
        match value {
            LuaValue::UserData(ud) => ud.borrow::<LuaBoolMatrix>().map(|m| m.clone()),
            _ => Err(LuaError::FromLuaConversionError {
                from: value.type_name(),
                to: "BoolMatrix".to_string(),
                message: Some("expected a BoolMatrix userdata".to_string()),
            }),
        }
    }
}

impl LuaUserData for LuaBoolMatrix {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("name", |_, this, ()| Ok(this.inner.name().to_string()));

        methods.add_method("get", |_, this, indices: LuaTable| {
            let idx = lua_indices(&indices)?;
            this.inner
                .try_get(&idx)
                .map(|a| LuaAtom { inner: a })
                .map_err(|e| LuaError::RuntimeError(e.to_string()))
        });

        methods.add_method("atoms", |lua, this, ()| {
            let t = lua.create_table()?;
            for (i, atom) in this.inner.atoms().iter().enumerate() {
                t.set(i + 1, LuaAtom { inner: *atom })?;
            }
            Ok(t)
        });
    }
}

/// The Lua-facing builder.  Wraps an `Option<PuzzleBuilder>` so `build()`
/// can consume the inner builder; subsequent calls error rather than panic.
pub struct LuaBuilder {
    inner: Arc<Mutex<Option<PuzzleBuilder>>>,
}

impl LuaBuilder {
    fn with_mut<R>(&self, f: impl FnOnce(&mut PuzzleBuilder) -> LuaResult<R>) -> LuaResult<R> {
        let mut guard = self.inner.lock().unwrap();
        let b = guard.as_mut().ok_or_else(|| {
            LuaError::RuntimeError("Builder has already been consumed by build()".to_string())
        })?;
        f(b)
    }
}

impl LuaUserData for LuaBuilder {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("kind", |_, this, name: String| {
            this.with_mut(|b| {
                b.kind(name);
                Ok(())
            })
        });

        methods.add_method("info", |_, this, s: String| {
            this.with_mut(|b| {
                b.info(s);
                Ok(())
            })
        });

        methods.add_method(
            "var_bool_matrix",
            |_, this, (name, dims): (String, LuaTable)| {
                let dims = lua_dims(&dims)?;
                this.with_mut(|b| {
                    Ok(LuaBoolMatrix {
                        inner: b.var_bool_matrix(&name, &dims),
                    })
                })
            },
        );

        methods.add_method(
            "con_bool_matrix",
            |_, this, (name, dims): (String, LuaTable)| {
                let dims = lua_dims(&dims)?;
                this.with_mut(|b| {
                    Ok(LuaBoolMatrix {
                        inner: b.con_bool_matrix(&name, &dims),
                    })
                })
            },
        );

        methods.add_method(
            "aux_bool_matrix",
            |_, this, (name, dims): (String, LuaTable)| {
                let dims = lua_dims(&dims)?;
                this.with_mut(|b| {
                    Ok(LuaBoolMatrix {
                        inner: b.aux_bool_matrix(&name, &dims),
                    })
                })
            },
        );

        methods.add_method("con_bool", |_, this, name: String| {
            this.with_mut(|b| {
                Ok(LuaAtom {
                    inner: b.con_bool(&name),
                })
            })
        });

        methods.add_method("aux_bool", |_, this, name: String| {
            this.with_mut(|b| {
                Ok(LuaAtom {
                    inner: b.aux_bool(&name),
                })
            })
        });

        methods.add_method(
            "reveal_bool_matrix",
            |_, this, (name, dims): (String, LuaTable)| {
                let dims = lua_dims(&dims)?;
                this.with_mut(|b| {
                    Ok(LuaBoolMatrix {
                        inner: b.reveal_bool_matrix(&name, &dims),
                    })
                })
            },
        );

        methods.add_method("reveal", |_, this, (src, target): (String, String)| {
            this.with_mut(|b| {
                b.set_reveal(&src, &target)
                    .map_err(|e| LuaError::RuntimeError(e.to_string()))
            })
        });

        methods.add_method("show", |_, this, (var, role): (String, String)| {
            let role = parse_show_role(&role)?;
            this.with_mut(|b| {
                b.show(&var, role);
                Ok(())
            })
        });

        methods.add_method(
            "sum_ge",
            |_, this, (guard, signed, k, family, description): (LuaAtom, LuaTable, i64, String, String)| {
                let signed = lua_signed_list(&signed)?;
                this.with_mut(|b| {
                    b.sum_ge(guard.inner, &signed, k, &family, description)
                        .map(|_| ())
                        .map_err(|e| LuaError::RuntimeError(e.to_string()))
                })
            },
        );

        methods.add_method(
            "sum_le",
            |_, this, (guard, signed, k, family, description): (LuaAtom, LuaTable, i64, String, String)| {
                let signed = lua_signed_list(&signed)?;
                this.with_mut(|b| {
                    b.sum_le(guard.inner, &signed, k, &family, description)
                        .map(|_| ())
                        .map_err(|e| LuaError::RuntimeError(e.to_string()))
                })
            },
        );

        methods.add_method(
            "sum_eq",
            |_, this, (guard, signed, k, family, description): (LuaAtom, LuaTable, i64, String, String)| {
                let signed = lua_signed_list(&signed)?;
                this.with_mut(|b| {
                    b.sum_eq(guard.inner, &signed, k, &family, description)
                        .map(|_| ())
                        .map_err(|e| LuaError::RuntimeError(e.to_string()))
                })
            },
        );

        methods.add_method(
            "sum_eq_unguarded",
            |_, this, (signed, k): (LuaTable, i64)| {
                let signed = lua_signed_list(&signed)?;
                this.with_mut(|b| {
                    b.sum_eq_unguarded(&signed, k)
                        .map_err(|e| LuaError::RuntimeError(e.to_string()))
                })
            },
        );

        methods.add_method("build", |_, this, ()| {
            let mut guard = this.inner.lock().unwrap();
            let b = guard
                .take()
                .ok_or_else(|| LuaError::RuntimeError("Builder already consumed".to_string()))?;
            let puzzle = b
                .build()
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            Ok(LuaPuzzle {
                inner: Arc::new(puzzle),
            })
        });
    }
}

/// Build a `Vec<i64>` of 1-indexed indices from a Lua sequence.
fn lua_indices(t: &LuaTable) -> LuaResult<Vec<i64>> {
    let mut out = Vec::with_capacity(t.len()? as usize);
    for v in t.clone().sequence_values::<i64>() {
        out.push(v?);
    }
    Ok(out)
}

/// Build a `Vec<RangeInclusive<i64>>` from a Lua sequence of `{first, last}`
/// pairs.  An empty outer table yields an empty `Vec`, which the builder
/// treats as a 0-d (scalar) declaration.
fn lua_dims(t: &LuaTable) -> LuaResult<Vec<RangeInclusive<i64>>> {
    let mut out = Vec::new();
    for pair in t.clone().sequence_values::<LuaTable>() {
        let pair = pair?;
        let lo: i64 = pair.get(1)?;
        let hi: i64 = pair.get(2)?;
        if lo > hi {
            return Err(LuaError::RuntimeError(format!(
                "matrix dimension {lo}..{hi}: first must be <= last"
            )));
        }
        out.push(lo..=hi);
    }
    Ok(out)
}

fn lua_signed_list(t: &LuaTable) -> LuaResult<Vec<Signed>> {
    let mut out = Vec::with_capacity(t.len()? as usize);
    for v in t.clone().sequence_values::<LuaSigned>() {
        out.push(v?.inner);
    }
    Ok(out)
}

fn parse_show_role(s: &str) -> LuaResult<ShowRole> {
    match s {
        "main" => Ok(ShowRole::Main),
        "cages" => Ok(ShowRole::Cages),
        "region_tint" => Ok(ShowRole::RegionTint),
        "givens" => Ok(ShowRole::Givens),
        "cage_sums" => Ok(ShowRole::CageSums),
        "less_than" => Ok(ShowRole::LessThan),
        "side_labels" => Ok(ShowRole::SideLabels),
        other => Err(LuaError::RuntimeError(format!(
            "unknown show role '{other}' (supported: main, cages, region_tint, givens, cage_sums, less_than, side_labels)"
        ))),
    }
}

fn new_builder(_lua: &Lua, _: ()) -> LuaResult<LuaBuilder> {
    Ok(LuaBuilder {
        inner: Arc::new(Mutex::new(Some(PuzzleBuilder::new()))),
    })
}

/// Create the `Builder` class table for Lua.
pub fn create_builder_class(lua: &Lua) -> LuaResult<LuaTable> {
    let class = lua.create_table()?;
    class.set("new", lua.create_function(new_builder)?)?;
    Ok(class)
}
