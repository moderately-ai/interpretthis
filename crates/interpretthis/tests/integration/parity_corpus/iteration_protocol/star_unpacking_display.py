# Star-unpacking inside list/tuple/set displays splats the iterable's items.
a = [1, 2]
b = (3, 4)
print([*a, *b])
print([0, *a, 5, *b])
print((*a, *b))
print({*a, *b, 2})
print([*range(3), *"xy"])
print([*{1, 2}])
d = {"k": 9}
print([*d])                      # iterating a dict yields its keys
