# Pins: augmented assignment into a list slice.
a = [1, 2, 3, 4, 5]
a[1:4] += [9, 9]
print(a)
b = [0, 1, 2, 3]
b[::2] = [7, 8]  # plain slice assign still works
print(b)
c = [1, 2, 3]
c[1:2] += [4, 5]
print(c)
