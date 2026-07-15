lst = list(range(10))
print(lst[2:5])
print(lst[::2])
print(lst[::-1])
print(lst[-3:])
print(lst[:-3])
print(lst[1:8:2])
print(lst[-1:-5:-1])
print("hello world"[::2])
print("hello"[::-1])
print((1,2,3,4,5)[1:4])
lst2 = list(range(5))
lst2[1:3] = [10, 20, 30]
print(lst2)
lst3 = list(range(5))
del lst3[1:3]
print(lst3)
lst4 = list(range(10))
lst4[::2] = [0]*5
print(lst4)
print(lst[5:2])
print(lst[100:])
s = "abcdef"
print(s[1:-1])
