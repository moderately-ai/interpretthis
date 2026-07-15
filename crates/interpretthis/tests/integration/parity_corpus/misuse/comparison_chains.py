x = 5
print(1 < x < 10)
print(1 < x > 3)
print("a" < "b" < "c")
print(1 == 1.0 == True)
lst = [1, 2, 3]
print([1, 2] < [1, 2, 3])
print((1, 2) < (1, 3))
print(min(3, 1, 2), max([4, 2, 8]))
print(sorted([3, 1, 2], key=lambda x: -x))
print(all(x > 0 for x in lst), any(x > 2 for x in lst))
