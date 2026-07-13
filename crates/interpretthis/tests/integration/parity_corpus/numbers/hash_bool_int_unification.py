# hash(True) == hash(1) and hash(False) == hash(0). CPython invariant
# (bool is an int subclass), required for the dict-key collapse to behave
# correctly. The numeric unification chain extends to floats: hash(1.0) == hash(1).
print(hash(True) == hash(1))
print(hash(False) == hash(0))
print(hash(1) == hash(1.0))
print(hash(True) == hash(1.0))
