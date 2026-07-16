// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `collections.UserDict` / `UserList` / `UserString` — the delegating wrapper
//! base classes. They are pure-Python in CPython, and the interpreter has no
//! standalone bootstrap, so they are defined here as Python source and lazily
//! evaluated into the class registry the first time one is imported (see
//! `eval_import_from`). Each wraps a `.data` attribute and forwards the
//! container protocol to it, so user subclasses that override a single method
//! and call `super()` behave as they do in CPython.
//!
//! `__missing__` support (which CPython's `UserDict.__getitem__` reaches via
//! `self.__class__`) is omitted as a separate parity gap. The results are built
//! with `type(self)(...)` — the idiomatic equivalent of `self.__class__(...)`
//! (both now resolve, since `__class__` reads alias `type(x)`).

use crate::{error::EvalError, state::InterpreterState, tools::Tools, value::Value};

/// Whether `name` is one of the lazily-bootstrapped `collections` classes.
pub fn is_user_collection(name: &str) -> bool {
    matches!(name, "UserDict" | "UserList" | "UserString")
}

fn source(name: &str) -> Option<&'static str> {
    match name {
        "UserDict" => Some(USER_DICT_SRC),
        "UserList" => Some(USER_LIST_SRC),
        "UserString" => Some(USER_STRING_SRC),
        _ => None,
    }
}

/// Ensure the class `name` is registered in the class registry, evaluating its
/// Python source on first use. Binds the class name in the current scope (as a
/// module-level `class` statement would) — the caller then binds the requested
/// import target. Idempotent: a second import is a no-op.
pub async fn ensure_registered(
    state: &mut InterpreterState,
    name: &str,
    tools: &Tools,
) -> Result<(), EvalError> {
    if state.classes.contains_key(name) {
        return Ok(());
    }
    let Some(src) = source(name) else {
        return Ok(());
    };
    // The methods reference the base class name (`isinstance(x, UserList)`) and
    // build results with `type(self)(...)` — the idiomatic form (equivalent to
    // `self.__class__(...)`); a module-level class binding plus the live-globals
    // closure make both resolve while the methods run.
    let stmts = crate::parser::parse(src).map_err(EvalError::Interpreter)?;
    let saved = std::mem::replace(&mut state.current_source, src.to_string());
    let outcome = crate::eval::eval_body(state, &stmts, tools).await;
    state.current_source = saved;
    outcome.map(|_| ())
}

/// Register the class (if needed) and bind the requested import target.
/// Mirrors `from collections import <name> [as <target>]`.
pub async fn import_binding(
    state: &mut InterpreterState,
    name: &str,
    target: &str,
    tools: &Tools,
) -> Result<(), EvalError> {
    let newly = !state.classes.contains_key(name);
    ensure_registered(state, name, tools).await?;
    state.set_variable(target, Value::Class(name.to_string())).map_err(EvalError::Interpreter)?;
    // On an aliased first import (`import UserDict as UD`), don't leak the plain
    // name that `ensure_registered`'s module-level class binding introduced.
    if newly && target != name {
        state.variables.remove(name);
    }
    Ok(())
}

const USER_DICT_SRC: &str = r#"
class UserDict:
    def __init__(self, dict=None, **kwargs):
        self.data = {}
        if dict is not None:
            self.update(dict)
        if kwargs:
            self.update(kwargs)
    def __len__(self):
        return len(self.data)
    def __getitem__(self, key):
        if key in self.data:
            return self.data[key]
        raise KeyError(key)
    def __setitem__(self, key, item):
        self.data[key] = item
    def __delitem__(self, key):
        del self.data[key]
    def __iter__(self):
        return iter(self.data)
    def __contains__(self, key):
        return key in self.data
    def __repr__(self):
        return repr(self.data)
    def __eq__(self, other):
        if isinstance(other, UserDict):
            return self.data == other.data
        return self.data == other
    def __ne__(self, other):
        return not (self == other)
    def keys(self):
        return self.data.keys()
    def values(self):
        return self.data.values()
    def items(self):
        return self.data.items()
    def get(self, key, default=None):
        return self.data.get(key, default)
    def pop(self, key, *args):
        return self.data.pop(key, *args)
    def popitem(self):
        return self.data.popitem()
    def setdefault(self, key, default=None):
        return self.data.setdefault(key, default)
    def clear(self):
        self.data.clear()
    def update(self, other=None, **kwargs):
        if other is not None:
            if hasattr(other, "keys"):
                for k in other.keys():
                    self.data[k] = other[k]
            else:
                for k, v in other:
                    self.data[k] = v
        for k in kwargs:
            self.data[k] = kwargs[k]
    def copy(self):
        return type(self)(dict(self.data))
"#;

const USER_LIST_SRC: &str = r#"
class UserList:
    def __init__(self, initlist=None):
        self.data = []
        if initlist is not None:
            if isinstance(initlist, UserList):
                self.data[:] = initlist.data[:]
            elif isinstance(initlist, list):
                self.data[:] = initlist
            else:
                self.data = list(initlist)
    def __repr__(self):
        return repr(self.data)
    def __len__(self):
        return len(self.data)
    def __getitem__(self, i):
        if isinstance(i, slice):
            return type(self)(self.data[i])
        return self.data[i]
    def __setitem__(self, i, item):
        self.data[i] = item
    def __delitem__(self, i):
        del self.data[i]
    def __contains__(self, item):
        return item in self.data
    def __iter__(self):
        return iter(self.data)
    def __add__(self, other):
        if isinstance(other, UserList):
            return type(self)(self.data + other.data)
        elif isinstance(other, list):
            return type(self)(self.data + other)
        return type(self)(self.data + list(other))
    def __mul__(self, n):
        return type(self)(self.data * n)
    def __eq__(self, other):
        if isinstance(other, UserList):
            return self.data == other.data
        return self.data == other
    def __ne__(self, other):
        return not (self == other)
    def __lt__(self, other):
        if isinstance(other, UserList):
            return self.data < other.data
        return self.data < other
    def append(self, item):
        self.data.append(item)
    def insert(self, i, item):
        self.data.insert(i, item)
    def pop(self, i=-1):
        return self.data.pop(i)
    def remove(self, item):
        self.data.remove(item)
    def clear(self):
        self.data.clear()
    def count(self, item):
        return self.data.count(item)
    def index(self, item, *args):
        return self.data.index(item, *args)
    def reverse(self):
        self.data.reverse()
    def sort(self, *args, **kwds):
        self.data.sort(*args, **kwds)
    def extend(self, other):
        if isinstance(other, UserList):
            self.data.extend(other.data)
        else:
            self.data.extend(other)
    def copy(self):
        return type(self)(self)
"#;

const USER_STRING_SRC: &str = r#"
class UserString:
    def __init__(self, seq):
        if isinstance(seq, str):
            self.data = seq
        elif isinstance(seq, UserString):
            self.data = seq.data[:]
        else:
            self.data = str(seq)
    def __str__(self):
        return str(self.data)
    def __repr__(self):
        return repr(self.data)
    def __len__(self):
        return len(self.data)
    def __getitem__(self, index):
        return type(self)(self.data[index])
    def __contains__(self, char):
        if isinstance(char, UserString):
            char = char.data
        return char in self.data
    def __add__(self, other):
        if isinstance(other, UserString):
            return type(self)(self.data + other.data)
        elif isinstance(other, str):
            return type(self)(self.data + other)
        return type(self)(self.data + str(other))
    def __mul__(self, n):
        return type(self)(self.data * n)
    def __eq__(self, other):
        if isinstance(other, UserString):
            return self.data == other.data
        return self.data == other
    def __ne__(self, other):
        return not (self == other)
    def __lt__(self, other):
        if isinstance(other, UserString):
            return self.data < other.data
        return self.data < other
    def __iter__(self):
        return iter(self.data)
    def upper(self):
        return type(self)(self.data.upper())
    def lower(self):
        return type(self)(self.data.lower())
    def capitalize(self):
        return type(self)(self.data.capitalize())
    def title(self):
        return type(self)(self.data.title())
    def swapcase(self):
        return type(self)(self.data.swapcase())
    def strip(self, chars=None):
        return type(self)(self.data.strip(chars))
    def lstrip(self, chars=None):
        return type(self)(self.data.lstrip(chars))
    def rstrip(self, chars=None):
        return type(self)(self.data.rstrip(chars))
    def split(self, sep=None, maxsplit=-1):
        return self.data.split(sep, maxsplit)
    def rsplit(self, sep=None, maxsplit=-1):
        return self.data.rsplit(sep, maxsplit)
    def splitlines(self, keepends=False):
        return self.data.splitlines(keepends)
    def join(self, seq):
        return self.data.join(seq)
    def replace(self, old, new, maxsplit=-1):
        if isinstance(old, UserString):
            old = old.data
        if isinstance(new, UserString):
            new = new.data
        return type(self)(self.data.replace(old, new, maxsplit))
    def startswith(self, prefix, start=0, end=None):
        return self.data.startswith(prefix, start, end if end is not None else len(self.data))
    def endswith(self, suffix, start=0, end=None):
        return self.data.endswith(suffix, start, end if end is not None else len(self.data))
    def find(self, sub, start=0, end=None):
        if isinstance(sub, UserString):
            sub = sub.data
        return self.data.find(sub, start, end if end is not None else len(self.data))
    def rfind(self, sub, start=0, end=None):
        if isinstance(sub, UserString):
            sub = sub.data
        return self.data.rfind(sub, start, end if end is not None else len(self.data))
    def index(self, sub, start=0, end=None):
        if isinstance(sub, UserString):
            sub = sub.data
        return self.data.index(sub, start, end if end is not None else len(self.data))
    def count(self, sub, start=0, end=None):
        if isinstance(sub, UserString):
            sub = sub.data
        return self.data.count(sub, start, end if end is not None else len(self.data))
    def center(self, width, *args):
        return type(self)(self.data.center(width, *args))
    def ljust(self, width, *args):
        return type(self)(self.data.ljust(width, *args))
    def rjust(self, width, *args):
        return type(self)(self.data.rjust(width, *args))
    def zfill(self, width):
        return type(self)(self.data.zfill(width))
    def format(self, *args, **kwargs):
        return self.data.format(*args, **kwargs)
    def encode(self, encoding="utf-8", errors="strict"):
        return self.data.encode(encoding, errors)
    def isalpha(self):
        return self.data.isalpha()
    def isdigit(self):
        return self.data.isdigit()
    def isalnum(self):
        return self.data.isalnum()
    def isspace(self):
        return self.data.isspace()
    def isupper(self):
        return self.data.isupper()
    def islower(self):
        return self.data.islower()
"#;
