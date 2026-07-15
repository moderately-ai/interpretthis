lst = list(range(10))
print(lst[2:8:2])
print(lst[::-1])
print(lst[-3:])
print(lst[:3])
lst[2:5] = [20, 30]
print(lst)
del lst[::2]
print(lst)
s = "hello world"
print(s[::2], s[6:], s[-5:])
t = (1, 2, 3, 4, 5)
print(t[1:4], t[::-1])
b = bytearray(b"abcdef")
b[1:3] = b"XY"
print(bytes(b))
