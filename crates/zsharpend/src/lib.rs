//! **Z#++** — StonedGem's scripting language: Sam-astro's Z# (MIT), kept
//! compatible and greatly extended, sandboxed for a browser. See
//! docs/ZSHARP.md for the language.
//!
//! * `.zsp` — programs ([`program::Program`]).
//! * `.zsx` — headers: declarations checked against the program.
//! * `.zsg` — GPU code: compute kernels and fragment shaders, compiled to
//!   WGSL ([`zsg::Module::validate`]) and runnable on the CPU.
//! * 2D drawing with [`canvas::Canvas`].

pub mod ast;
pub mod canvas;
pub mod extend;
pub mod interp;
pub mod lexer;
pub mod parser;
pub mod program;
pub mod stdlib;
pub mod value;
pub mod zsg;

pub use interp::{Interp, Limits};
pub use program::Program;
pub use value::Value;
