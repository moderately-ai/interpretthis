# `del list[start:stop:step]` honours the step. Regression: the step was ignored,
# so `del a[::2]` emptied the list (behaved like `del a[:]`).
a = [0, 1, 2, 3, 4, 5, 6, 7]
del a[::2]
print(a)                    # [1, 3, 5, 7]

b = [0, 1, 2, 3, 4, 5, 6, 7]
del b[1::2]
print(b)                    # [0, 2, 4, 6]

c = [0, 1, 2, 3, 4, 5]
del c[::-2]
print(c)                    # [0, 2, 4]

d = [0, 1, 2, 3, 4, 5]
del d[1:4]                  # step 1 contiguous still works
print(d)                    # [0, 4, 5]

e = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
del e[2:8:3]
print(e)                    # removes indices 2 and 5

# step 0 is a ValueError.
try:
    f = [1, 2, 3]
    del f[::0]
except ValueError:
    print("ValueError")
