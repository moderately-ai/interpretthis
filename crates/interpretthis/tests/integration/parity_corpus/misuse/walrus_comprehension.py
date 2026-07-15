data = [1, 2, 3, 4, 5]
print([y for x in data if (y := x * 2) > 4])
results = [(x, y) for x in range(3) if (y := x ** 2) < 5]
print(results)
n = 0
while (n := n + 1) < 4:
    print(n)
total = 0
nums = [1, 2, 3]
print([total := total + n for n in nums])
