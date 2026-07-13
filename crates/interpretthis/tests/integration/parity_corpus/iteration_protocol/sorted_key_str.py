# Pins: `sorted(xs, key=str)` — builtin name as key function. Common idiom
# for "sort ints lexicographically". The `__builtin__str` sentinel leak
# means today's interpreter passes a String into the key dispatch and
# errors "'str' object is not callable".
print(sorted([1, 10, 2, 20], key=str))
