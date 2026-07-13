# Attribute writes on builtin types raise AttributeError matching CPython.
# Pins types::dispatch_setattr -> None slot -> AttributeError shape.
# A6 closes a pre-existing divergence where dict accepted attribute
# writes as string-key inserts; CPython rejects all such writes.
for value in [[1, 2, 3], (1, 2), "abc", b"abc", {1, 2}, {"a": 1}, 42, 3.14, None]:
    try:
        value.foo = 9
        print("no error")
    except AttributeError:
        print("AttributeError")
