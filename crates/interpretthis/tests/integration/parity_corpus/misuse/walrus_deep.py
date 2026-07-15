if (n := 10) > 5:
    print(n)
data = [1, 2, 3, 4, 5]
print([y for x in data if (y := x * 2) > 4])
total = 0
values = [1, 2, 3]
while values and (v := values.pop()):
    total += v
print(total)
print([(z := i, z * 2) for i in range(3)])
