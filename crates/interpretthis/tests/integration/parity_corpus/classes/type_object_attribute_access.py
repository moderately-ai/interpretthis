# Attribute access on a builtin TYPE object resolves its methods and dunders as
# unbound descriptors, so hasattr/getattr agree with a real dispatch and an
# unknown name raises rather than fabricating a method.
for t in [float, int, str, list, dict, tuple, bool, bytes]:
    print(t.__name__, hasattr(t, "__format__"), hasattr(t, "__eq__"), hasattr(t, "upper"))

print(hasattr(str, "upper"), hasattr(list, "append"), hasattr(dict, "get"))
print(hasattr(int, "bit_length"), hasattr(int, "to_bytes"), hasattr(float, "is_integer"))
print(hasattr(str, "fakemethodxyz"), hasattr(list, "nope"))
print(hasattr(object, "__init__"), hasattr(object, "__new__"))

# getattr with a default falls back for an unknown attribute.
print(getattr(str, "fakemethodxyz", "DEFAULT"))
print(getattr(str, "upper"))

# Unbound builtin methods are callable and carry CPython's descriptor type name.
print(str.upper("hello"))
print(list(map(str.strip, [" a ", " b "])))
lst = [1, 2]
list.append(lst, 3)
print(lst)
print(int.bit_length(255))
print(type(str.upper).__name__, type(list.append).__name__)
print(type(dict.fromkeys).__name__, type(int.from_bytes).__name__)

# An unknown method on a type object raises with the "type object" phrasing.
try:
    str.fakemethodxyz("x")
except AttributeError as e:
    print(e)
