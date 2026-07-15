a = b = c = 0
print(a, b, c)
x, y = y, x = 1, 2
print(x, y)
d = {}
d["a"] = d["b"] = 5
print(d)
lst = [0] * 3
lst[0], lst[2] = 10, 20
print(lst)
total = 0
for i in range(5):
    total += i
print(total)
n = 10
n //= 3
n **= 2
print(n)
data = {"count": 0}
data["count"] += 5
print(data["count"])
