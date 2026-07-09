# Pin: hash(<float>) returns a stable platform-specific int matching CPython's
# _Py_HashDouble algorithm. CPython 3.12 on 64-bit: hash(1.5) -> 1152921504606846977.
print(hash(1.5))
