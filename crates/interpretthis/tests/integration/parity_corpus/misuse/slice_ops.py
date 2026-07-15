lst = [0, 1, 2, 3, 4, 5]
print(lst[::-1])
print(lst[1:5:2])
print(lst[::2])
print(lst[-2:])
print(lst[:-2])
print(lst[10:])
lst[1:3] = [10, 20, 30]
print(lst)
lst2 = [1, 2, 3, 4, 5, 6]
lst2[::2] = [7, 8, 9]
print(lst2)
del lst2[1:3]
print(lst2)
s = "hello"
print(s[::-1])
print(s[1:4])
