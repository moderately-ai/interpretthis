# User-listed gap: {True: x}[1] should return x because True == 1 in Python.
# CPython treats bool as an int subclass, so True and 1 share a hash and
# resolve to the same dict slot. Pre-A1, interpretthis's ValueKey distinguished
# Bool from Int so this raised KeyError.
print({True: "yes"}[1])
print({1: "one", True: "two"})  # True collapses to 1, prints {1: 'two'}
print({False: "no"}[0])
print({0: "zero", False: "f"})  # False collapses to 0
