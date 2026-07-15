# Extended slicing, negative steps, slice assignment, del.
lst = list(range(10))
print(lst[2:8], lst[::-1], lst[::2], lst[8:2:-1])
print(lst[-3:], lst[:-3], lst[-8:-2:2])
lst[2:5] = [20, 30]
print(lst)
lst[::2] = [0] * len(lst[::2])
print(lst)
del lst[::3]
print(lst)
s = "abcdefgh"
print(s[::-1], s[1:6:2], s[::3])
t = (0, 1, 2, 3, 4, 5)
print(t[::-1], t[1::2])
print(list(range(20))[5:15:3])
# slice object
sl = slice(1, 10, 2)
print(list(range(20))[sl], sl.indices(20))
