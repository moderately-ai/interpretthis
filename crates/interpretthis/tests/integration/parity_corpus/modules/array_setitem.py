from array import array

a = array("i", [1, 2, 3, 4])
# Item assignment mutates in place, preserving the typecode.
a[0] = 100
a[-1] = 40
print(a[0], a[-1], a.typecode)
print(list(a))
# Membership tests over the shared handle.
print(100 in a, 999 in a)
a.remove(100)
print(100 in a, list(a))
# Slice read still works alongside assignment.
a[1:3] = array("i", [7, 8, 9])
print(list(a))
b = array("d", [1.5, 2.5])
b[0] = 3.5
print(list(b), b.typecode)
