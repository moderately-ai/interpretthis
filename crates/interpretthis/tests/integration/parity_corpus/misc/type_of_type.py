# `type(x)` on a type object is the metaclass `type`, not the object's own
# builtin-callable label. A builtin *function* stays
# `builtin_function_or_method`.

print(type(int), type(str), type(list), type(dict))
print(type(tuple), type(set), type(frozenset), type(bool), type(float), type(bytes))
print(type(type), type(object))
print(type(int).__name__, type(str).__name__)
print(str(type(int)))

# Builtin functions are not types.
print(type(len), type(print), type(sorted), type(isinstance))

# Exception classes are types.
print(type(ValueError), type(Exception), type(KeyError))

# User classes and their metaclass.
class C:
    pass


print(type(C), type(C).__name__)
print(type(int) is type(str))
print(type(int) is type)
print(type(C) is type)

# Modules and instances for contrast.
import math

print(type(math))
print(type(math.pi), type(1), type(1.0), type("x"), type([]))
