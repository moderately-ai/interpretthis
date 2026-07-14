# Calling the result of an expression directly — `f()()`, a lambda literal, a
# partial — dispatches the computed callable. Regression: a non-name/non-method
# call target fell through to a name lookup with an empty name and raised
# NameError.
print((lambda x: x + 1)(5))
print((lambda a, b: a * b)(3, 4))

from functools import partial


def add(a, b):
    return a + b


print(partial(add, 10)(5))
print(partial(add, b=2)(3))


def make_adder(n):
    return lambda x: x + n


print(make_adder(100)(5))       # f()() — call the returned closure

# A list/dict of callables, indexed then called.
ops = [lambda: "zero", lambda: "one"]
print(ops[1]())
print({"k": lambda v: v * 2}["k"](21))
