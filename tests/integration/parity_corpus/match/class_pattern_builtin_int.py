# class_pattern_builtin_int: PEP 634 builtins take a single
# positional pattern that matches the whole value. Pins the
# builtin_single_positional branch in match_class.
def categorize(v):
    match v:
        case int(0):
            return "zero int"
        case int(x):
            return f"int {x}"
        case str(s):
            return f"str '{s}'"
        case float(f):
            return f"float {f}"
        case bool(b):
            # Note: bool is checked AFTER int in CPython (bool is a
            # subclass of int), so this arm is unreachable in CPython —
            # putting it after int verifies our order matches.
            return f"bool {b}"
        case list(items):
            return f"list len={len(items)}"
        case _:
            return "other"

print(categorize(0))
print(categorize(42))
print(categorize("hello"))
print(categorize(3.14))
print(categorize([1, 2, 3]))
print(categorize(None))
# Bool falls through to int because bool IS an int — CPython behaviour.
print(categorize(True))
