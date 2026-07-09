# Pins: `f = int; f("42")` — builtin stored under a fresh name, then called.
# Tests the eval_call variable-lookup branch for sentinel callables.
f = int
print(f("42"))
