# Pins: starred (`*`) unpacking in assignment targets — `a, *b, c`
# binds `b` to the middle slice. Heavy customer pattern for
# heterogeneous tuple processing.
a, *b, c = [1, 2, 3, 4, 5]
print(a, b, c)
*x, y = [10, 20, 30]
print(x, y)
first, *rest = "hello"
print(first, rest)
