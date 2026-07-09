# `True in [1]` is True because True == 1 per the bool↔int unification.
# Same for `1 in [True]`. Pins that dispatch_contains routes equality
# through dispatch_eq so the unification holds inside `in` too.
print(True in [1, 2, 3])
print(1 in [True, False])
print(False in (0, 1))
print(0 in (True,))
print(1.0 in [1])
print(True in {1: "yes"})  # dict membership uses key equality
