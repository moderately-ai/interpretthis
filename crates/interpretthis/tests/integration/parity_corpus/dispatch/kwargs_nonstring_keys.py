# `f(**mapping)` with a non-string key raises TypeError: keywords must be strings.
# Regression: the ** unpacking matched only `ValueKey::String` keys and silently
# dropped anything else, so `f(**{1: 2})` quietly passed no arguments.
def f(**kw):
    return sorted(kw.items())


try:
    f(**{1: 2})
    print("int-key NO ERROR")
except TypeError:
    print("int-key TypeError")

try:
    f(**{"a": 1, 2: 3})
    print("mixed NO ERROR")
except TypeError:
    print("mixed TypeError")

# String keys still unpack normally.
print(f(**{"a": 1, "b": 2}))
print(f(**{}))
