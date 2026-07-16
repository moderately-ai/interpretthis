# array supports + (concat, matching typecodes) and * int (repetition).
from array import array

a = array("i", [1, 2])
b = array("i", [3, 4])
print((a + b).tolist())
print((a * 3).tolist())
print((array("i", []) + a).tolist())
print((a * 0).tolist())
c = array("d", [1.5, 2.5])
print((c + array("d", [3.5])).tolist())
try:
    a + array("d", [1.0])
except TypeError:
    print("typecode mismatch")
try:
    a + [5, 6]
except TypeError:
    print("not an array")
