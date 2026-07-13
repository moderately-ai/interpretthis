# Pins: callable() returns True for every callable shape CPython
# accepts -- not just Function/Lambda. Today our `callable` builtin
# returns False for bound methods, builtin names, ModuleFunctions,
# unbound type methods, and class objects.
d = {'a': 1}
print(callable(d.get))           # True  (bound method)
print(callable(len))             # True  (builtin)
print(callable(str.upper))       # True  (unbound type method)
print(callable(int))             # True  (type / class)
print(callable(42))              # False (instance of non-callable)
print(callable("hi"))            # False
