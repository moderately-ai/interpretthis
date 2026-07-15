# Bound builtin methods expose __self__, __name__, __qualname__.
print((255).bit_length.__name__, (255).bit_length.__qualname__, (255).bit_length.__self__)
print("hello".upper.__name__, "hello".upper.__qualname__, "hello".upper.__self__)
print([1, 2].append.__name__, [1, 2].append.__qualname__)
print(complex(1, 2).conjugate.__self__, complex(1, 2).conjugate.__qualname__)
print((3.14).is_integer.__name__, (3.14).is_integer.__qualname__)
print(b"abc".hex.__name__, b"abc".hex.__qualname__)

# Place receiver (bound off a variable) resolves __self__ live.
x = [1, 2, 3]
f = x.append
print(f.__self__, f.__name__, f.__qualname__)
d = {"a": 1, "b": 2}
k = d.keys
print(k.__self__, k.__qualname__)
ba = bytearray(b"xy")
print(ba.append.__self__, ba.append.__qualname__)

# Unbound type methods (descriptors): __name__/__qualname__ but no __self__.
print(str.upper.__name__, str.upper.__qualname__)
print(list.append.__qualname__, dict.get.__qualname__)
try:
    str.upper.__self__
except AttributeError:
    print("descriptor has no __self__")

# A method with a mutated receiver reflects the current value.
lst = [1]
g = lst.append
lst.append(99)
print(g.__self__)
