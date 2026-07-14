# isinstance() walks the builtin exception hierarchy. Regression: it matched only
# the exact type or "Exception", so `isinstance(KeyError(), LookupError)` was
# False.
print(isinstance(KeyError(), LookupError))
print(isinstance(IndexError(), LookupError))
print(isinstance(ZeroDivisionError(), ArithmeticError))
print(isinstance(KeyError(), Exception))
print(isinstance(KeyError(), KeyError))

# Not subclasses.
print(isinstance(KeyError(), ValueError))
print(isinstance(ValueError(), LookupError))

# The except machinery already agreed; isinstance now matches it.
try:
    raise KeyError("k")
except LookupError:
    print("caught as LookupError")
