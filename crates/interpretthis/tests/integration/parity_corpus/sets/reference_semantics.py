# Sets are reference types: assignment and argument-passing share one object,
# in-place mutators and augmented operators mutate that shared object (aliases
# see it, identity is preserved), while the binary operators return a new set.
a = {1, 2, 3}
b = a
b.add(4)
print(sorted(a), a is b)

# Mutating through a function argument is visible to the caller.
def grow(s):
    s.add(99)


grow(a)
print(sorted(a))


# Augmented operators mutate in place (same identity), unlike the binary form.
s = {1, 2, 3}
t = s
before = id(s)
s |= {4, 5}
print(sorted(s), sorted(t), s is t, id(s) == before)

s &= {2, 3, 4, 99}
print(sorted(s), sorted(t))

s -= {2}
print(sorted(s), sorted(t))

s ^= {3, 7, 8}
print(sorted(s), sorted(t))

# The binary operators build a NEW set (identity changes, alias unaffected).
u = {1, 2, 3}
v = u
u = u | {4}
print(sorted(u), sorted(v), u is v)

# In-place order matches CPython's update semantics, not copy semantics: print
# unsorted to observe slot order after an in-place merge on the live table.
w = {1, 9, 17, 25, 3}
w |= {2, 9, 18, 25, 4, 100}
print(list(w))

# frozenset has no in-place slot: `fz |= x` rebinds to a fresh frozenset.
fz = frozenset({1, 2, 3})
original = fz
fz |= {4, 5}
print(sorted(fz), sorted(original), fz is original)
print(type(fz).__name__)
