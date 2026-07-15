lst = [1, 2, 3]
lst.insert(1, 99)
print(lst)
lst[1:1] = [7, 8]
print(lst)
print([1, 2, 3] * 3)
print([0] * 5)
a = [1, 2, 3]
a.extend(range(4, 7))
print(a)
print(list(range(10, 0, -2)))
print([x for x in [1, 2, 3, 4] if x % 2][0])
b = [[1, 2], [3, 4]]
print([item for sublist in b for item in sublist])
c = [1, 2, 3, 4, 5]
print(c[::2], c[1::2], c[::-1])
c[::2] = [10, 30, 50]
print(c)
del c[::2]
print(c)
print(max([[1, 2], [3, 0], [2, 5]]))
print(sorted([[2, 1], [1, 2], [1, 1]]))
