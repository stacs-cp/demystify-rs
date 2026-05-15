//! # Demystify
//!
//! A constraint satisfaction solver that provides human-readable explanations
//! for puzzle solutions. Demystify uses MUS (Minimal Unsatisfiable Subset)
//! computation to find the smallest set of constraints that justify each
//! deduction step.
//!
//! ## Overview
//!
//! Demystify works by:
//! 1. Converting puzzle definitions (in Essence Prime format) to CNF (SAT) formulas
//! 2. Finding which variable assignments can be logically deduced
//! 3. Computing minimal explanations for each deduction using MUS algorithms
//! 4. Presenting step-by-step solutions with human-readable constraint names
//!
//! ## Architecture
//!
//! The library is organized into several modules:
//!
//! - [`problem`] - Core data structures and solving logic
//!   - [`problem::parse`] - Parsing Essence Prime and DIMACS files
//!   - [`problem::solver`] - SAT solving and MUS computation
//!   - [`problem::planner`] - Step-by-step solution planning
//!   - [`problem::serialize`] - JSON serialization for pre-parsed puzzles
//! - [`satcore`] - Low-level SAT solver wrapper (rustsat-glucose, rustsat-cadical, rustsat-batsat; wasm32 uses BatSat only)
//! - [`json`] - JSON puzzle representation utilities
//! - [`web`] - SVG/HTML output generation
//!
//! ## Basic Usage
//!
//! ```rust,no_run
//! use demystify::problem::parse::PuzzleParse;
//! use demystify::problem::solver::PuzzleSolver;
//! use demystify::problem::planner::PuzzlePlanner;
//! use std::sync::Arc;
//!
//! // Load a pre-parsed puzzle from JSON
//! let puzzle = PuzzleParse::load_from_json("sudoku.json".as_ref()).unwrap();
//! let puzzle = Arc::new(puzzle);
//!
//! // Create a solver and planner
//! let solver = PuzzleSolver::new(puzzle).unwrap();
//! let mut planner = PuzzlePlanner::new(solver);
//!
//! // Solve step by step
//! let solution = planner.quick_solve();
//! for (step_num, step) in solution.iter().enumerate() {
//!     println!("Step {}:", step_num + 1);
//!     for um in step {
//!         println!("  Deduced: {:?}", um.lits);
//!         println!("  Using: {:?}", um.constraints);
//!         if let Some(name) = &um.name {
//!             println!("  Technique: {}", name);
//!         }
//!     }
//! }
//! ```
//!
//! ## Puzzle Preparation
//!
//! Before using this library, puzzles must be prepared using the `demystify` CLI
//! with the `--save-parsed` option:
//!
//! ```bash
//! demystify --model puzzle.eprime --param instance.param --save-parsed puzzle.json
//! ```
//!
//! This converts the Essence Prime definition to a pre-parsed JSON format that
//! can be loaded quickly without requiring the Conjure toolchain.
//!
//! ## Key Types
//!
//! - [`problem::PuzVar`] - A puzzle variable (e.g., `grid[1,2]`)
//! - [`problem::PuzLit`] - A literal (variable assignment like `grid[1,2]=5`)
//! - [`problem::parse::PuzzleParse`] - A fully parsed puzzle ready for solving
//! - [`problem::solver::PuzzleSolver`] - The SAT-based constraint solver
//! - [`problem::planner::PuzzlePlanner`] - Plans solution steps with explanations
//!
//! ## Supported Puzzle Types
//!
//! Demystify supports any puzzle that can be expressed in Essence Prime, including:
//! - Sudoku and variants (Killer, Miracle, X-Sudoku)
//! - Binairo (binary puzzles)
//! - Minesweeper
//! - Star Battle
//! - Kakuro
//! - Futoshiki
//! - And many more...
//!
//! ## Example: Analyzing Solution Difficulty
//!
//! ```rust,no_run
//! use demystify::problem::parse::PuzzleParse;
//! use demystify::problem::solver::PuzzleSolver;
//! use demystify::problem::planner::PuzzlePlanner;
//! use std::sync::Arc;
//!
//! let puzzle = PuzzleParse::load_from_json("puzzle.json".as_ref()).unwrap();
//! let solver = PuzzleSolver::new(Arc::new(puzzle)).unwrap();
//! let mut planner = PuzzlePlanner::new(solver);
//!
//! // Get difficulty ratings for all deductions
//! let muses = planner.all_muses_with_larger();
//! for (lit, mus_set) in muses.muses() {
//!     if let Some(min_len) = mus_set.iter().map(|mc| mc.mus_len()).min() {
//!         println!("Deduction requires {} constraints", min_len);
//!     }
//! }
//! ```
//!
//! ## Features
//!
//! - **Incremental solving**: Efficiently reuses SAT solver state
//! - **Parallel MUS computation**: Uses rayon for parallel constraint analysis
//! - **Multiple MUS algorithms**: `Quick`, `Slice`, `Cake`, and `Dynamic` (the default); see [`problem::solver::Strategy`]
//! - **Serialization**: Save/load parsed puzzles as JSON for fast startup
//! - **HTML/SVG output**: Generate visual step-by-step explanations

#![allow(dead_code)]

pub mod json;
pub mod named_strategy;
pub mod problem;
pub mod satcore;
pub mod snapshot;
pub mod stats;
pub mod walkthrough;
pub mod web;
