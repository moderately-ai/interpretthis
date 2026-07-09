# hash(small int) returns the int itself (modulo the -1 → -2 sentinel rule).
# Pins int_hash_slot against CPython's long_hash output.
print(hash(0))
print(hash(1))
print(hash(42))
print(hash(-1))   # -1 sentinel: CPython returns -2
print(hash(-42))
