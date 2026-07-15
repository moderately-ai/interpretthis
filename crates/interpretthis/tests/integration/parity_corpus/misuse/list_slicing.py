lst = list(range(10))
print(lst[2:5])
print(lst[:3])
print(lst[7:])
print(lst[::2])
print(lst[1::2])
print(lst[::-1])
print(lst[-3:])
print(lst[:-3])
print(lst[-5:-2])
print(lst[10:20])
print(lst[5:2])
print(lst[::3])
print(lst[8:2:-1])
lst2 = list(range(5))
lst2[1:3] = [10, 20, 30]
print(lst2)
lst3 = list(range(6))
lst3[::2] = ["a", "b", "c"]
print(lst3)
del lst3[1:3]
print(lst3)
s = "abcdefg"
print(s[::2], s[1:6:2], s[::-1])
