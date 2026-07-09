// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-type method dispatch.
//!
//! `dispatch_method` in this module is the central routing hub. It
//! takes a receiver Value, a method name, and an argument list, and
//! routes to the type-specific dispatcher in one of the child modules
//! below. Each child module owns the full method surface for one
//! Python type.
//!
//! Routing happens by `Value` variant; methods that don't belong to the
//! receiver's type return an `AttributeError` from the per-type
//! dispatcher (CPython's wording). User-class instance methods are
//! handled separately by `eval::classes::dispatch_instance_method` —
//! this hub only covers builtin types.

pub(crate) mod bytes;
pub(crate) mod counter;
pub(crate) mod deque;
pub(crate) mod dict;
pub(crate) mod int;
pub(crate) mod list;
pub(crate) mod set;
pub(crate) mod str;
pub(crate) mod tuple;
