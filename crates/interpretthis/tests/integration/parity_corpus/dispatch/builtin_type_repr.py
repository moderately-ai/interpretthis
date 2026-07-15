# Builtin type objects repr as <class 'name'>, not <built-in function name>.
print(int, str, list, dict, set, tuple, bool, float)
print(bytes, bytearray, frozenset, range, type, object)
print([int, str], (list, dict))
print(repr(int), repr(list))
print(str(int) == "<class 'int'>")
# True functions still repr as <built-in function ...>
print(len, print, sorted, abs)
