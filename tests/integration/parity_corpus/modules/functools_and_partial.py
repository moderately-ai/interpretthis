# Pins: functools.reduce with/without initial; functools.partial
# binds positional args.
from functools import reduce, partial

print(reduce(lambda a, b: a + b, [1, 2, 3, 4, 5]))
print(reduce(lambda a, b: a * b, [1, 2, 3, 4], 10))

add = lambda x, y: x + y
add5 = partial(add, 5)
print(add5(3))
