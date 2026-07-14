l = [3, 1, 4, 1, 5, 9, 2, 6]
l.sort()
print(l)
l.sort(reverse=True)
print(l)
print(l.index(5), l.count(1))
l.insert(0, 99)
print(l[:3])
l.remove(99)
print(l.pop(), l.pop(0))
l2 = [1, 2, 3]
l2.extend([4, 5])
l2 += [6]
print(l2)
print(l2[::-1], l2[1::2])
l3 = list(range(5))
l3[1:3] = [10, 20, 30]
print(l3)
del l3[0]
print(l3)
print([1, 2, 3] + [4, 5], [0] * 3)
