# bool is an int subclass: True hashes as 1, False as 0, so they collide/dedup
# with 1/0 and land in the same slot. Order and dedup must match CPython.
print(list({True, 1, False, 0}))
print(list({0, False, 1, True, 2}))
print(list({False, 0, 0.0}))
print(list({True, 1, 1.0, 1 + 0j}))
print({1, 2, 3} == {True, 2, 3})
print(list({5, True, 10, False, 3}))
print(len({True, 1, 1.0}), len({False, 0, 0.0, 0j}))
# Mixed bool into a larger set to force resize with the collisions present.
print(list({x for x in [True, False, 1, 0, 2, 3, 4, 5, 6, 7, 8, 9, 10]}))
